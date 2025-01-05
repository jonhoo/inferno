use quick_xml::{
    events::{attributes::Attributes, Event},
    reader::Reader,
};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    io::{self, BufRead},
    sync::Arc,
};

use super::{
    common::{fix_partially_demangled_rust_symbol, Occurrences},
    Collapse,
};

const REF: &[u8] = b"ref";
const ID: &[u8] = b"id";
const NAME: &[u8] = b"name";
const TRACE_QUERY_RESULT: &[u8] = b"trace-query-result";
const NODE: &[u8] = b"node";
const ROW: &[u8] = b"row";
const BACKTRACE: &[u8] = b"backtrace";
const FRAME: &[u8] = b"frame";

/// Context of collapsing a xctrace's `Time Profiler` xml
#[derive(Default)]
struct CollapseContext {
    /// xml tag backtrace
    state_backtrace: TagBacktrace,
    // --------- per-xml caches below -----------
    /// backtrace_id <--> BackTrace
    backtraces: BTreeMap<u64, Arc<Backtrace>>,
    /// backtrace_id <--> Frame
    frames: BTreeMap<u64, Arc<Frame>>,
}

// Note that sometimes same Backtrace have different id, we need to merge them
// before writing folded file.
struct BacktraceOccurrences {
    /// How many times the backtrace occurred.
    num: u64,
    /// Backtrace content
    backtrace: Arc<Backtrace>,
}

/// Backtrace of scanned xml tags.
#[derive(Default)]
struct TagBacktrace {
    backtrace: Vec<CurrentTag>,
}

impl TagBacktrace {
    fn push_back(&mut self, tag: CurrentTag) {
        self.backtrace.push(tag);
    }

    /// If `name` matches the top tag of the stack, the tag is popped.
    fn pop_with_name(&mut self, name: &[u8]) -> Option<CurrentTag> {
        if self
            .backtrace
            .last()
            .map(|t| t.matches(name))
            .unwrap_or_default()
        {
            // There is at least one element in backtrace, hence this unwrap is safe.
            Some(self.backtrace.pop().unwrap())
        } else {
            None
        }
    }

    fn top_mut(&mut self) -> Option<&mut CurrentTag> {
        self.backtrace.last_mut()
    }
}

/// The tag we are scanning, with additional states.
enum CurrentTag {
    TraceQueryResult(TraceQueryResultState),
    Node(NodeState),
    Row(RowState),
    Backtrace(BacktraceState),
    Frame(FrameState),
    Other(String),
}

impl CurrentTag {
    fn matches(&self, name: &[u8]) -> bool {
        match name {
            TRACE_QUERY_RESULT => matches!(self, Self::TraceQueryResult(_)),
            NODE => matches!(self, Self::Node(_)),
            ROW => matches!(self, Self::Row(_)),
            BACKTRACE => matches!(self, Self::Backtrace(_)),
            FRAME => matches!(self, Self::Frame(_)),
            other => matches!(self, Self::Other(tag) if tag.as_bytes() == other),
        }
    }
}

#[derive(Default)]
struct TraceQueryResultState {
    nodes: Vec<Node>,
}

#[derive(Default)]
struct NodeState {
    rows: Vec<Row>,
}

#[derive(Default)]
struct RowState {
    backtrace: Option<Arc<Backtrace>>,
}

#[derive(Default)]
struct BacktraceState {
    id: u64,
    frames: Vec<Arc<Frame>>,
}

#[derive(Default)]
struct FrameState {
    id: u64,
    name: Vec<u8>,
}

struct Node {
    rows: Vec<Row>,
}

struct Row {
    backtrace: Arc<Backtrace>,
}

struct Backtrace {
    id: u64,
    frames: Vec<Arc<Frame>>,
}

struct Frame {
    id: u64,
    name: Vec<u8>,
}

impl Backtrace {
    fn to_folded(&self) -> Vec<u8> {
        let mut folded = Vec::new();
        let mut first = Some(());
        // Because stack frames are arranged from top to bottom in xctrace's
        // output, here we use `.rev(`.
        for frame in self.frames.iter().rev() {
            if first.take().is_none() {
                folded.push(b';');
            }
            let frame_name = String::from_utf8_lossy(&frame.name);
            let frame_name = fix_partially_demangled_rust_symbol(&frame_name);
            folded.extend(frame_name.as_bytes().iter().copied());
        }
        folded
    }
}

/// Unescapes the text in xml exported from xctrace.
fn unescape_xctrace_text(text: Cow<'_, [u8]>) -> io::Result<Vec<u8>> {
    // xctrace shouldn't give us invalid xml text here, therefore
    // we don't expect the error branch being hit:
    //
    // `quick_xml::escape::unescape` will error out if the input is not a valid xml text:
    // https://github.com/tafia/quick-xml/blob/0793d6a8d006cb5dabf66bf2a25ddbf198305b46/src/escape.rs#L253
    match quick_xml::escape::unescape(&String::from_utf8_lossy(&text)) {
        Ok(x) => Ok(x.into_owned().into_bytes()),
        Err(e) => invalid_data_error!(
            "Invalid xml text from xctrace, which is not expected: {:?}",
            e
        ),
    }
}

fn get_u64_from_attributes(key: &'static [u8], attributes: &Attributes) -> io::Result<u64> {
    let id = attributes
        .clone()
        .filter_map(|x| x.ok())
        .find_map(|x| (x.key.into_inner() == key).then_some(x.value));
    let Some(id) = id else {
        return invalid_data_error!("No {} found in attributes", String::from_utf8_lossy(key));
    };
    let id = String::from_utf8_lossy(&id);
    match id.parse() {
        Ok(x) => Ok(x),
        Err(e) => invalid_data_error!(
            "Unrecognized {}: {}: {:?}",
            String::from_utf8_lossy(key),
            id,
            e
        ),
    }
}

fn get_name_from_attributes(attributes: &Attributes) -> io::Result<Vec<u8>> {
    let name = attributes
        .clone()
        .filter_map(|x| x.ok())
        .find_map(|x| (x.key.into_inner() == NAME).then_some(x.value));
    match name {
        Some(x) => unescape_xctrace_text(x),
        None => invalid_data_error!("No name(symbol) found in attributes"),
    }
}

/// Extract necessary info from attributes for constructing backtrace.
fn attributes_to_backtrace(attributes: &Attributes) -> io::Result<u64> {
    get_u64_from_attributes(ID, attributes)
}

/// Extract necessary info from attributes for constructing frame.
fn attributes_to_frame(attributes: &Attributes) -> io::Result<(u64, Vec<u8>)> {
    let id = get_u64_from_attributes(ID, attributes)?;
    let name = get_name_from_attributes(attributes)?;
    Ok((id, name))
}

/// A stack collapser for the output of `xctrace export`.
#[derive(Default)]
pub struct Folder;

impl Collapse for Folder {
    fn collapse<R, W>(&mut self, reader: R, writer: W) -> std::io::Result<()>
    where
        R: std::io::BufRead,
        W: std::io::Write,
    {
        let mut reader = Reader::from_reader(reader);
        self.collapse_inner(&mut reader, writer)
    }

    fn is_applicable(&mut self, input: &str) -> Option<bool> {
        let mut input = input.as_bytes();
        let mut line = String::new();
        loop {
            if let Ok(n) = input.read_line(&mut line) {
                if n == 0 {
                    break;
                }
            } else {
                return Some(false);
            }

            let trimmed = line.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.contains(r#"<?xml version="1.0"?>"#));
            }
            line.clear();
        }
        None
    }
}

impl Folder {
    fn collapse_inner<R, W>(&mut self, reader: &mut Reader<R>, writer: W) -> io::Result<()>
    where
        R: std::io::BufRead,
        W: std::io::Write,
    {
        let mut context = CollapseContext::default();
        let mut buf = Vec::new();
        let nodes = loop {
            let event = match reader.read_event_into(&mut buf) {
                Ok(x) => x,
                Err(e) => return invalid_data_error!("Read xml event failed: {:?}", e),
            };
            match event {
                Event::Start(start) => {
                    let attributes = start.attributes();
                    let name = start.name().into_inner();
                    let new_state = match (context.state_backtrace.top_mut(), name) {
                        (None, TRACE_QUERY_RESULT) => {
                            CurrentTag::TraceQueryResult(TraceQueryResultState::default())
                        }
                        (Some(CurrentTag::TraceQueryResult(_)), NODE) => {
                            let node_state = NodeState { rows: Vec::new() };
                            CurrentTag::Node(node_state)
                        }
                        (Some(CurrentTag::Node(_)), ROW) => {
                            let row_state = RowState { backtrace: None };
                            CurrentTag::Row(row_state)
                        }
                        (Some(CurrentTag::Row(_)), BACKTRACE) => {
                            let id = attributes_to_backtrace(&attributes)?;
                            CurrentTag::Backtrace(BacktraceState {
                                id,
                                frames: Vec::new(),
                            })
                        }
                        (Some(CurrentTag::Backtrace(_)), FRAME) => {
                            let (id, name) = attributes_to_frame(&attributes)?;
                            CurrentTag::Frame(FrameState { id, name })
                        }
                        _ => CurrentTag::Other(String::from_utf8_lossy(name).into_owned()),
                    };
                    context.state_backtrace.push_back(new_state);
                }
                Event::End(end) => {
                    let name = end.name().into_inner();
                    let Some(state) = context.state_backtrace.pop_with_name(name) else {
                        return invalid_data_error!(
                            "Unpaired tag: {}",
                            String::from_utf8_lossy(name)
                        );
                    };
                    match (context.state_backtrace.top_mut(), state) {
                        (None, CurrentTag::TraceQueryResult(trace_query_result_state)) => {
                            let TraceQueryResultState { nodes } = trace_query_result_state;
                            break nodes;
                        }
                        (
                            Some(CurrentTag::TraceQueryResult(trace_query_result_state)),
                            CurrentTag::Node(node_state),
                        ) => {
                            let NodeState { rows } = node_state;
                            trace_query_result_state.nodes.push(Node { rows });
                        }
                        (Some(CurrentTag::Node(node_state)), CurrentTag::Row(row_state)) => {
                            let RowState { backtrace } = row_state;
                            // <backtrace/> in some row is replaced with <sentinel/>, hence we ignore thess rows.
                            if let Some(backtrace) = backtrace {
                                node_state.rows.push(Row { backtrace });
                            }
                        }
                        (
                            Some(CurrentTag::Row(row_state)),
                            CurrentTag::Backtrace(backtrace_state),
                        ) => {
                            let BacktraceState { id, frames } = backtrace_state;
                            let backtrace = Arc::new(Backtrace { id, frames });
                            let ret = context.backtraces.insert(backtrace.id, backtrace.clone());
                            assert!(ret.is_none());
                            row_state.backtrace = Some(backtrace);
                        }
                        (
                            Some(CurrentTag::Backtrace(backtrace_state)),
                            CurrentTag::Frame(frame_state),
                        ) => {
                            let FrameState { id, name } = frame_state;
                            let frame = Arc::new(Frame { id, name });
                            let ret = context.frames.insert(frame.id, frame.clone());
                            assert!(ret.is_none());
                            backtrace_state.frames.push(frame);
                        }
                        _ => {}
                    }
                }
                Event::Empty(empty) => {
                    let attributes = empty.attributes();
                    let name = empty.name().into_inner();
                    match (context.state_backtrace.top_mut(), name) {
                        (Some(CurrentTag::Row(row_state)), BACKTRACE) => {
                            let backtrace =
                                if let Ok(ref_id) = get_u64_from_attributes(REF, &attributes) {
                                    match context.backtraces.get(&ref_id) {
                                        Some(x) => x.clone(),
                                        None => {
                                            return invalid_data_error!(
                                                "Invalid backtrace ref id: {}",
                                                ref_id
                                            )
                                        }
                                    }
                                } else if let Ok(id) = attributes_to_backtrace(&attributes) {
                                    let backtrace = Arc::new(Backtrace {
                                        id,
                                        frames: Vec::new(),
                                    });
                                    context.backtraces.insert(id, backtrace.clone());
                                    backtrace
                                } else {
                                    return invalid_data_error!(
                                        "Get ref_id or attributes of backtrace failed."
                                    );
                                };
                            row_state.backtrace = Some(backtrace);
                        }
                        (Some(CurrentTag::Backtrace(backtrace_state)), FRAME) => {
                            let frame =
                                if let Ok(ref_id) = get_u64_from_attributes(REF, &attributes) {
                                    match context.frames.get(&ref_id) {
                                        Some(x) => x.clone(),
                                        None => {
                                            return invalid_data_error!(
                                                "Invalid frame ref id: {}",
                                                ref_id
                                            )
                                        }
                                    }
                                } else if let Ok((id, name)) = attributes_to_frame(&attributes) {
                                    let frame = Arc::new(Frame { id, name });
                                    context.frames.insert(id, frame.clone());
                                    frame
                                } else {
                                    return invalid_data_error!(
                                        "Get ref_id or attributes of frame failed."
                                    );
                                };
                            backtrace_state.frames.push(frame);
                        }
                        _ => {}
                    }
                }
                Event::Text(_)
                | Event::Comment(_)
                | Event::CData(_)
                | Event::Decl(_)
                | Event::PI(_)
                | Event::DocType(_) => {}
                Event::Eof => return invalid_data_error!("Unexpected EOF"),
            }
        };

        let backtraces = nodes
            .into_iter()
            .flat_map(|Node { rows }| rows)
            .map(|Row { backtrace }| backtrace);

        // backtrace_id <--> BacktraceOccurrences
        let mut frames: BTreeMap<u64, BacktraceOccurrences> = BTreeMap::new();
        for backtrace in backtraces {
            let frame = frames
                .entry(backtrace.id)
                .or_insert_with(|| BacktraceOccurrences { num: 0, backtrace });
            frame.num += 1;
        }

        let mut occurrences = Occurrences::new(1);

        for frame in frames.into_values() {
            let BacktraceOccurrences { num, backtrace } = frame;
            let folded = backtrace.to_folded();
            occurrences.insert_or_add(String::from_utf8_lossy(&folded).into_owned(), num as usize);
        }
        occurrences.write_and_clear(writer)
    }
}
