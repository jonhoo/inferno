//! Collapser of xctrace-exported xml files.
use quick_xml::{
    events::{attributes::Attributes, Event},
    reader::Reader,
};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    io::{self, BufRead},
};

use super::{
    common::{fix_partially_demangled_rust_symbol, Occurrences},
    Collapse,
};

/* A simplified xctrace xml example:

```xml
<?xml version="1.0"?>
<trace-query-result>
<node xpath='//trace-toc[1]/run[1]/data[1]/table[11]'>
    <row>
        <backtrace id="10">
            <frame id="11" name="0x18d3df0f1" addr="0x18d3df0f1"></frame>
            <frame id="13" name="start" addr="0x18d373904"></frame>
        </backtrace>
    </row>
    <row>
        <backtrace id="15">
            <frame id="16" name="dyld4::prepare(dyld4::APIs&amp;, dyld3::MachOAnalyzer const*)" addr="0x18d374c1d"></frame>
            <frame id="17" name="start" addr="0x18d373dc4"></frame>
        </backtrace>
    </row>
    <row>
        <backtrace ref="15"/>
    </row>
    <row>
        <backtrace id="20">
            <frame id="21" name="rust_test2::foo::ha31fba0d06a8a3eb" addr="0x102af5d99"></frame>
            <frame id="24" name="rust_test2::main::h2640131654657f56" addr="0x102af5f30"></frame>
            <frame id="25" name="std::sys_common::backtrace::__rust_begin_short_backtrace::h4f1b05744198b1bb" addr="0x102af5d04"></frame>
            <frame id="27" name="std::rt::lang_start::_$u7b$$u7b$closure$u7d$$u7d$::h7d0ebd26afb1a225" addr="0x102af5d1c"></frame>
            <frame id="29" name="std::rt::lang_start_internal::hfc27b745d167a74d" addr="0x102b0a324"></frame>
            <frame id="30" name="main" addr="0x102af5fa0"></frame>
            <frame id="31" name="start" addr="0x18d373e50"></frame>
        </backtrace>
    </row>
    <row>
        <backtrace id="39">
            <frame id="40" name="rust_test2::foo::ha31fba0d06a8a3eb" addr="0x102af5d95"></frame>
            <frame ref="24"/>
            <frame ref="25"/>
            <frame ref="27"/>
            <frame ref="29"/>
            <frame ref="30"/>
            <frame ref="31"/>
        </backtrace></row>
</node>
</trace-query-result>
```
 */

// ----------- attribute names -----------

/// Reference to a previous declared backtrace/frame, it's value is id.
const REF: &[u8] = b"ref";
/// Id of a backtrace/frame
const ID: &[u8] = b"id";
/// Symbolicated name of a frame
const NAME: &[u8] = b"name";

// -----------    tag names    -----------

/// Root tag of xctrace's xml output
const TRACE_QUERY_RESULT: &[u8] = b"trace-query-result";
/// Container of a sample set
const NODE: &[u8] = b"node";
/// Container of a sample.
const ROW: &[u8] = b"row";
/// Stack backtrace
const BACKTRACE: &[u8] = b"backtrace";
/// Stack frame
const FRAME: &[u8] = b"frame";

// Is this a tag we are interested in?
fn is_interested_tag(tag: &[u8]) -> bool {
    matches!(tag, TRACE_QUERY_RESULT | NODE | ROW | BACKTRACE | FRAME)
}

// xctrace's sample backtrace is address-based. Two identical backtraces might
// have different addresses. Therefore, same Backtrace could have different id,
// we need to merge them before writing folded file.
//
// For example:
//
// ```rust
// fn foo() {
//     bar(); // 1
//     bar(); // 2
// }
// ```
//
// Then there will be two identical symbolized backtraces with different address set:
//
// ```xml
// <backtrace id="20">
//   <frame id="21" name="rust_test2::bar::h2640131654657f56" addr="0x102af5c30"></frame>
//   <frame id="22" name="rust_test2::foo::ha31fba0d06a8a3eb" addr="0x102af5d95"></frame>
// </backtrace>
// ```
//
// ```xml
// <backtrace id="23">
//   <frame ref="21"/>
//   <frame id="24" name="rust_test2::foo::ha31fba0d06a8a3eb" addr="0x102af5d99"></frame>
// </backtrace>
// ```
struct BacktraceOccurrences {
    /// How many times the backtrace occurred.
    num: usize,
    /// Backtrace content
    backtrace: BacktraceId,
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
        self.backtrace
            .last()
            .is_some_and(|t| t.matches(name))
            .then(|| self.backtrace.pop())?
    }

    fn top_mut(&mut self) -> Option<&mut CurrentTag> {
        self.backtrace.last_mut()
    }
}

/// The tag we are scanning, with additional states.
enum CurrentTag {
    TraceQueryResult {
        nodes: Vec<Node>,
    },
    Node {
        rows: Vec<Row>,
    },
    Row {
        backtrace: Option<BacktraceId>,
    },
    Backtrace {
        id: BacktraceId,
        frames: Vec<FrameId>,
    },
    Frame {
        id: FrameId,
        name: Box<[u8]>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
struct FrameId(u64);
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
struct BacktraceId(u64);

impl CurrentTag {
    fn matches(&self, name: &[u8]) -> bool {
        match name {
            TRACE_QUERY_RESULT => matches!(self, Self::TraceQueryResult { .. }),
            NODE => matches!(self, Self::Node { .. }),
            ROW => matches!(self, Self::Row { .. }),
            BACKTRACE => matches!(self, Self::Backtrace { .. }),
            FRAME => matches!(self, Self::Frame { .. }),
            _ => false,
        }
    }
}

struct Node {
    rows: Vec<Row>,
}

struct Row {
    backtrace: BacktraceId,
}

struct Backtrace {
    id: BacktraceId,
    frames: Vec<FrameId>,
}

struct Frame {
    id: FrameId,
    name: Box<[u8]>,
}

impl BacktraceId {
    fn resolve(&self, context: &Folder) -> String {
        let backtrace = context
            .backtraces
            .get(self)
            .expect("Backtrace id not registered in collapse context, this is a inferno bug.");
        let mut folded = String::new();
        let mut first = Some(());
        // Because stack frames are arranged from top to bottom in xctrace's
        // output, here we use `.rev(`.
        for frame in backtrace.frames.iter().rev() {
            if first.take().is_none() {
                folded.push(';');
            }
            let frame = context
                .frames
                .get(frame)
                .expect("Frame id not registered in collapse context, this is a inferno bug.");
            let frame_name = String::from_utf8_lossy(&frame.name);
            let frame_name = fix_partially_demangled_rust_symbol(&frame_name);
            folded.push_str(&frame_name);
        }
        folded
    }
}

/// Unescapes the text in xml exported from xctrace.
fn unescape_xctrace_text(text: Cow<'_, [u8]>) -> io::Result<Box<[u8]>> {
    // xctrace shouldn't give us invalid xml text here, therefore
    // we don't expect the error branch being hit:
    //
    // `quick_xml::escape::unescape` will error out if the input is not a valid xml text:
    // https://github.com/tafia/quick-xml/blob/0793d6a8d006cb5dabf66bf2a25ddbf198305b46/src/escape.rs#L253
    match quick_xml::escape::unescape(&String::from_utf8_lossy(&text)) {
        Ok(x) => Ok(x.into_owned().into_bytes().into_boxed_slice()),
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

fn get_name_from_attributes(attributes: &Attributes) -> io::Result<Box<[u8]>> {
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
fn attributes_to_backtrace(attributes: &Attributes) -> io::Result<BacktraceId> {
    get_u64_from_attributes(ID, attributes).map(BacktraceId)
}

/// Extract necessary info from attributes for constructing frame.
fn attributes_to_frame(attributes: &Attributes) -> io::Result<(FrameId, Box<[u8]>)> {
    let id = get_u64_from_attributes(ID, attributes)?;
    let name = get_name_from_attributes(attributes)?;
    Ok((FrameId(id), name))
}

/// Context of collapsing a xctrace's `Time Profiler` xml
#[derive(Default)]
pub struct Folder {
    /// xml tag backtrace
    state_backtrace: TagBacktrace,
    // --------- per-xml caches below -----------
    /// backtrace_id <--> BackTrace
    backtraces: BTreeMap<BacktraceId, Backtrace>,
    /// backtrace_id <--> Frame
    frames: BTreeMap<FrameId, Frame>,
}

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
        let mut is_xml = false;
        let mut is_xctrace = false;
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
                // Remove right bracket in pattern for possibility of adding attributes in the future.
                is_xml = is_xml || trimmed.contains(r#"<?xml version="1.0""#);
                is_xctrace = is_xctrace || trimmed.contains("<trace-query-result");
                if is_xml && is_xctrace {
                    return Some(true);
                }
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
                    let new_state = match (self.state_backtrace.top_mut(), name) {
                        (None, TRACE_QUERY_RESULT) => {
                            Some(CurrentTag::TraceQueryResult { nodes: Vec::new() })
                        }
                        (None, _) => {
                            // skip unknown root tags
                            None
                        }
                        (Some(CurrentTag::TraceQueryResult { .. }), NODE) => {
                            Some(CurrentTag::Node { rows: Vec::new() })
                        }
                        (Some(CurrentTag::Node { .. }), ROW) => {
                            Some(CurrentTag::Row { backtrace: None })
                        }
                        (Some(CurrentTag::Row { .. }), BACKTRACE) => {
                            let id = attributes_to_backtrace(&attributes)?;
                            Some(CurrentTag::Backtrace {
                                id,
                                frames: Vec::new(),
                            })
                        }
                        (Some(CurrentTag::Backtrace { .. }), FRAME) => {
                            let (id, name) = attributes_to_frame(&attributes)?;
                            Some(CurrentTag::Frame { id, name })
                        }
                        (Some(_), _) => {
                            // Skip tag combination we are not interested in.
                            // Tags' matchedness&validity have alreay been checked by quick_xml:
                            // - https://docs.rs/quick-xml/latest/quick_xml/reader/struct.Config.html#structfield.allow_unmatched_ends
                            // - https://docs.rs/quick-xml/latest/quick_xml/reader/struct.Config.html#structfield.check_end_names
                            None
                        }
                    };
                    if let Some(new_state) = new_state {
                        self.state_backtrace.push_back(new_state);
                    }
                }
                Event::End(end) => {
                    let name = end.name().into_inner();
                    // Skip unknown tags
                    if !is_interested_tag(name) {
                        continue;
                    }
                    let Some(state) = self.state_backtrace.pop_with_name(name) else {
                        return invalid_data_error!(
                            "Unpaired tag: {}",
                            String::from_utf8_lossy(name)
                        );
                    };
                    // Retrieve information when a tag span ends.
                    match (self.state_backtrace.top_mut(), state) {
                        (None, CurrentTag::TraceQueryResult { nodes }) => {
                            break nodes;
                        }
                        (
                            Some(CurrentTag::TraceQueryResult { nodes }),
                            CurrentTag::Node { rows },
                        ) => {
                            nodes.push(Node { rows });
                        }
                        (Some(CurrentTag::Node { rows }), CurrentTag::Row { backtrace }) => {
                            // <backtrace/> in some row is replaced with <sentinel/>, hence we ignore thess rows.
                            if let Some(backtrace) = backtrace {
                                rows.push(Row { backtrace });
                            }
                        }
                        (
                            Some(CurrentTag::Row { backtrace }),
                            CurrentTag::Backtrace { id, frames },
                        ) => {
                            let new_backtrace = Backtrace { id, frames };
                            let ret = self.backtraces.insert(new_backtrace.id, new_backtrace);
                            if ret.is_some() {
                                return invalid_data_error!(
                                    "Repeated backtrace id in xctrace output: {:?}",
                                    id
                                );
                            }
                            *backtrace = Some(id);
                        }
                        (
                            Some(CurrentTag::Backtrace { id: _, frames }),
                            CurrentTag::Frame { id, name },
                        ) => {
                            let frame = Frame { id, name };
                            let ret = self.frames.insert(frame.id, frame);
                            if ret.is_some() {
                                return invalid_data_error!(
                                    "Repeated frame id in xctrace output: {:?}",
                                    id
                                );
                            }
                            frames.push(id);
                        }
                        _ => unreachable!("Bad tag stack, this is a bug of inferno."),
                    }
                }
                Event::Empty(empty) => {
                    let attributes = empty.attributes();
                    let name = empty.name().into_inner();
                    match (self.state_backtrace.top_mut(), name) {
                        (Some(CurrentTag::Row { backtrace }), BACKTRACE) => {
                            let new_backtrace =
                                if let Ok(ref_id) = get_u64_from_attributes(REF, &attributes) {
                                    if !self.backtraces.contains_key(&BacktraceId(ref_id)) {
                                        return invalid_data_error!(
                                            "Invalid backtrace ref id: {}",
                                            ref_id
                                        );
                                    }
                                    BacktraceId(ref_id)
                                } else if let Ok(id) = attributes_to_backtrace(&attributes) {
                                    let backtrace = Backtrace {
                                        id,
                                        frames: Vec::new(),
                                    };
                                    let ret = self.backtraces.insert(id, backtrace);
                                    if ret.is_some() {
                                        return invalid_data_error!(
                                            "Repeated backtrace id in xctrace output: {:?}",
                                            id
                                        );
                                    }
                                    id
                                } else {
                                    return invalid_data_error!(
                                        "Get ref_id or attributes of backtrace failed."
                                    );
                                };
                            *backtrace = Some(new_backtrace);
                        }
                        (Some(CurrentTag::Backtrace { id: _, frames }), FRAME) => {
                            let frame = if let Ok(ref_id) =
                                get_u64_from_attributes(REF, &attributes)
                            {
                                if !self.frames.contains_key(&FrameId(ref_id)) {
                                    return invalid_data_error!("Invalid frame ref id: {}", ref_id);
                                }
                                FrameId(ref_id)
                            } else if let Ok((id, name)) = attributes_to_frame(&attributes) {
                                let frame = Frame { id, name };
                                let ret = self.frames.insert(id, frame);
                                if ret.is_some() {
                                    return invalid_data_error!(
                                        "Repeated frame id in xctrace output: {:?}",
                                        id
                                    );
                                }
                                id
                            } else {
                                return invalid_data_error!(
                                    "Get ref_id or attributes of frame failed."
                                );
                            };
                            frames.push(frame);
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
        let mut backtrace_occurrences: BTreeMap<BacktraceId, BacktraceOccurrences> =
            BTreeMap::new();
        for backtrace in backtraces {
            let frame = backtrace_occurrences
                .entry(backtrace)
                .or_insert_with(|| BacktraceOccurrences { num: 0, backtrace });
            frame.num += 1;
        }

        let mut occurrences = Occurrences::new(1);

        for BacktraceOccurrences { num, backtrace } in backtrace_occurrences.into_values() {
            occurrences.insert_or_add(backtrace.resolve(self), num);
        }
        occurrences.write_and_clear(writer)
    }
}
