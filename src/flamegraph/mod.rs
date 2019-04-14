macro_rules! args {
    ($($key:expr => $value:expr),*) => {{
        [$(($key, $value),)*].into_iter().map(|(k, v): &(&str, &str)| (*k, *v))
    }};
}

mod attrs;
pub mod color;
mod merge;
mod svg;

pub use attrs::FuncFrameAttrsMap;
pub use color::Palette;

use crate::flamegraph::color::{Color, SearchColor};
use attrs::FrameAttrs;
use log::{error, warn};
use num_format::Locale;
use quick_xml::{
    events::{BytesEnd, BytesStart, BytesText, Event},
    Writer,
};
use std::fs::File;
use std::io::prelude::*;
use std::io::{self, BufReader};
use std::iter;
use std::path::PathBuf;
use std::str::FromStr;
use str_stack::StrStack;
use svg::StyleOptions;

const XPAD: usize = 10; // pad lefm and right
const FRAMEPAD: usize = 1; // vertical padding for frames

/// Default values for [`Options`].
pub mod defaults {
    macro_rules! doc {
        ($str:expr, $($def:tt)*) => {
            #[doc = $str]
            $($def)*
        };
    }

    macro_rules! define {
        ($($name:ident : $t:ty = $val:tt),*) => {
            $(
                doc!(
                    stringify!($val),
                    pub const $name: $t = $val;
                );
            )*

            #[doc(hidden)]
            pub mod str {
                use lazy_static::lazy_static;
            $(
                lazy_static! {
                    pub static ref $name: String = ($val).to_string();
                }
            )*
            }
        }
    }

    define! {
        COLORS: &str = "hot",
        SEARCH_COLOR: &str = "#e600e6",
        TITLE: &str = "Flame Graph",
        IMAGE_WIDTH: usize = 1200,
        FRAME_HEIGHT: usize = 16,
        MIN_WIDTH: f64 = 0.1,
        FONT_TYPE: &str = "Verdana",
        FONT_SIZE: usize = 12,
        FONT_WIDTH: f64 = 0.59,
        COUNT_NAME: &str = "samples",
        NAME_TYPE: &str = "Function:",
        FACTOR: f64 = 1.0
    }
}

/// Configure the flame graph.
#[derive(Debug, PartialEq)]
pub struct Options<'a> {
    /// The color palette to use when plotting.
    pub colors: color::Palette,

    /// The background color for the plot.
    ///
    /// If `None`, the background color will be selected based on the value of `colors`.
    pub bgcolors: Option<color::BackgroundColor>,

    /// Choose names based on the hashes of function names.
    ///
    /// This will cause similar functions to be colored similarly.
    pub hash: bool,

    /// Store the choice of color for each function so that later invocations use the same colors.
    ///
    /// With this option enabled, a file called `palette.map` will be created the first time a
    /// flame graph is generated, and the color chosen for each function will be written into it.
    /// On subsequent invocations, functions that already have a color registered in that file will
    /// be given the stored color rather than be assigned a new one. New functions will have their
    /// colors persisted for future runs.
    ///
    /// This feature was first implemented [by Shawn
    /// Sterling](https://github.com/brendangregg/FlameGraph/pull/25).
    pub palette_map: Option<&'a mut color::PaletteMap>,

    /// Assign extra attributes to particular functions.
    ///
    /// In particular, if a function appears in the given map, it will have extra attributes set in
    /// the resulting SVG based on its value in the map.
    pub func_frameattrs: FuncFrameAttrsMap,

    /// Whether to plot a plot that grows top-to-bottom or bottom-up (the default).
    pub direction: Direction,

    /// The search color for flame graph.
    ///
    /// [Default value](defaults::SEARCH_COLOR).
    pub search_color: SearchColor,

    /// The title for the flame graph.
    ///
    /// [Default value](defaults::TITLE).
    pub title: String,

    /// The subtitle for the flame graph.
    ///
    /// Defaults to None.
    pub subtitle: Option<String>,

    /// Width of for the flame graph
    ///
    /// [Default value](defaults::IMAGE_WIDTH).
    pub image_width: usize,

    /// Height of each frame.
    ///
    /// [Default value](defaults::FRAME_HEIGHT).
    pub frame_height: usize,

    /// Minimal width to omit smaller functions
    ///
    /// [Default value](defaults::MIN_WIDTH).
    pub min_width: f64,

    /// The font type for the flame graph.
    ///
    /// [Default value](defaults::FONT_TYPE).
    pub font_type: String,

    /// Font size for the flame graph.
    ///
    /// [Default value](defaults::FONT_SIZE).
    pub font_size: usize,

    /// Font width for the flame graph.
    ///
    /// [Default value](defaults::FONT_WIDTH).
    pub font_width: f64,

    /// Count type label for the flame graph.
    ///
    /// [Default value](defaults::COUNT_NAME).
    pub count_name: String,

    /// Name type label for the flame graph.
    ///
    /// [Default value](defaults::NAME_TYPE).
    pub name_type: String,

    /// The notes for the flame graph.
    pub notes: String,

    /// By default, if [differential] samples are included in the provided stacks, the resulting
    /// flame graph will compute and show differentials as `sample#2 - sample#1`. If this option is
    /// set, the differential is instead computed using `sample#1 - sample#2`.
    ///
    /// [differential]: http://www.brendangregg.com/blog/2014-11-09/differential-flame-graphs.html
    pub negate_differentials: bool,

    /// Factor to scale sample counts by in the flame graph.
    ///
    /// This option can be useful if the sample data has fractional sample counts since the fractional
    /// parts are stripped off when creating the flame graph. To work around this you can scale up the
    /// sample counts to be integers, then scale them back down in the graph with the `factor` option.
    ///
    /// For example, if you have `23.4` as a sample count you can upscale it to `234`, then set `factor`
    /// to `0.1`.
    ///
    /// [Default value](defaults::FACTOR).
    pub factor: f64,

    /// Pretty print XML with newlines and indentation.
    pub pretty_xml: bool,

    /// Don't sort the input lines.
    ///
    /// If you know for sure that your folded stack lines are sorted you can set this flag to get
    /// a performance boost. If you have multiple input files, the lines will be merged and sorted
    /// regardless.
    ///
    /// Note that if you use `from_sorted_lines` directly, the it is always your responsibility to
    /// make sure the lines are sorted.
    pub no_sort: bool,

    /// Generate stack-reversed flame graph.
    ///
    /// Note that stack lines must always be sorted after reversing the stacks so the `no_sort`
    /// option will be ignored.
    pub reverse_stack_order: bool,

    /// Don't include static JavaScript in flame graph.
    /// This is only meant to be used in tests.
    #[doc(hidden)]
    pub no_javascript: bool,
}

impl<'a> Options<'a> {
    /// Calculate pad top, including title
    pub(super) fn ypad1(&self) -> usize {
        self.font_size * 3
    }

    /// Calculate pad bottom, including labels
    pub(super) fn ypad2(&self) -> usize {
        self.font_size * 2 + 10
    }
}

impl<'a> Default for Options<'a> {
    fn default() -> Self {
        Options {
            colors: Palette::from_str(defaults::COLORS).unwrap(),
            search_color: SearchColor::from_str(defaults::SEARCH_COLOR).unwrap(),
            title: defaults::TITLE.to_string(),
            image_width: defaults::IMAGE_WIDTH,
            frame_height: defaults::FRAME_HEIGHT,
            min_width: defaults::MIN_WIDTH,
            font_type: defaults::FONT_TYPE.to_string(),
            font_size: defaults::FONT_SIZE,
            font_width: defaults::FONT_WIDTH,
            count_name: defaults::COUNT_NAME.to_string(),
            name_type: defaults::NAME_TYPE.to_string(),
            factor: defaults::FACTOR,
            notes: Default::default(),
            subtitle: Default::default(),
            bgcolors: Default::default(),
            hash: Default::default(),
            palette_map: Default::default(),
            func_frameattrs: Default::default(),
            direction: Default::default(),
            negate_differentials: Default::default(),
            pretty_xml: Default::default(),
            no_sort: Default::default(),
            reverse_stack_order: Default::default(),
            no_javascript: Default::default(),
        }
    }
}

/// The direction the plot should grow.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Direction {
    /// Stacks grow from the bottom to the top.
    ///
    /// The `(all)` meta frame will be at the bottom.
    Straight,

    /// Stacks grow from the top to the bottom.
    ///
    /// The `(all)` meta frame will be at the top.
    Inverted,
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Straight
    }
}

struct Rectangle {
    x1: usize,
    y1: usize,
    x2: usize,
    y2: usize,
}
impl Rectangle {
    fn width(&self) -> usize {
        self.x2 - self.x1
    }
    fn height(&self) -> usize {
        self.y2 - self.y1
    }
}

/// Produce a flame graph from an iterator over folded stack lines.
///
/// This function expects each folded stack to contain the following whitespace-separated fields:
///
///  - A semicolon-separated list of frame names (e.g., `main;foo;bar;baz`).
///  - A sample count for the given stack.
///  - An optional second sample count.
///
/// If two sample counts are provided, a [differential flame graph] is produced. In this mode, the
/// flame graph uses the difference between the two sample counts to show how the sample counts for
/// each stack has changed between the first and second profiling.
///
/// The resulting flame graph will be written out to `writer` in SVG format.
///
/// [differential flame graph]: http://www.brendangregg.com/blog/2014-11-09/differential-flame-graphs.html
#[allow(clippy::cognitive_complexity)]
pub fn from_lines<'a, I, W>(opt: &mut Options<'_>, lines: I, writer: W) -> quick_xml::Result<()>
where
    I: IntoIterator<Item = &'a str>,
    W: Write,
{
    let mut reversed = StrStack::new();
    let (mut frames, time, ignored, delta_max) = if opt.reverse_stack_order {
        if opt.no_sort {
            warn!(
                "Input lines are always sorted when `reverse_stack_order` is `true`. \
                 The `no_sort` option is being ignored."
            );
        }
        // Reverse order of stacks and sort.
        let mut stack = String::new();
        for line in lines {
            stack.clear();
            let samples_idx = merge::rfind_samples(line)
                .map(|(i, _)| i)
                .unwrap_or_else(|| line.len());
            let samples_idx = merge::rfind_samples(&line[..samples_idx - 1])
                .map(|(i, _)| i)
                .unwrap_or(samples_idx);
            for (i, func) in line[..samples_idx].trim().split(';').rev().enumerate() {
                if i != 0 {
                    stack.push(';');
                }
                stack.push_str(func);
            }
            stack.push(' ');
            stack.push_str(&line[samples_idx..]);
            reversed.push(&stack);
        }
        let mut reversed: Vec<&str> = reversed.iter().collect();
        reversed.sort_unstable();
        merge::frames(reversed)?
    } else if opt.no_sort {
        // Lines don't need sorting.
        merge::frames(lines)?
    } else {
        // Sort lines by default.
        let mut lines: Vec<&str> = lines.into_iter().collect();
        lines.sort_unstable();
        merge::frames(lines)?
    };

    if ignored != 0 {
        warn!("Ignored {} lines with invalid format", ignored);
    }

    let mut buffer = StrStack::new();

    // let's start writing the svg!
    let mut svg = if opt.pretty_xml {
        Writer::new_with_indent(writer, b' ', 4)
    } else {
        Writer::new(writer)
    };

    if time == 0 {
        error!("No stack counts found");
        // emit an error message SVG, for tools automating flamegraph use
        let imageheight = opt.font_size * 5;
        svg::write_header(&mut svg, imageheight, &opt)?;
        svg::write_str(
            &mut svg,
            &mut buffer,
            svg::TextItem {
                x: (opt.image_width / 2) as f64,
                y: (opt.font_size * 2) as f64,
                text: "ERROR: No valid input provided to flamegraph".into(),
                extra: None,
            },
        )?;
        svg.write_event(Event::End(BytesEnd::borrowed(b"svg")))?;
        svg.write_event(Event::Eof)?;
        return Err(quick_xml::Error::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            "No stack counts found",
        )));
    }

    let timemax = time;
    let widthpertime = (opt.image_width - 2 * XPAD) as f64 / timemax as f64;
    let minwidth_time = opt.min_width / widthpertime;

    // prune blocks that are too narrow
    let mut depthmax = 0;
    frames.retain(|frame| {
        if ((frame.end_time - frame.start_time) as f64) < minwidth_time {
            false
        } else {
            depthmax = std::cmp::max(depthmax, frame.location.depth);
            true
        }
    });

    // draw canvas, and embed interactive JavaScript program
    let imageheight = ((depthmax + 1) * opt.frame_height) + opt.ypad1() + opt.ypad2();
    svg::write_header(&mut svg, imageheight, &opt)?;

    let (bgcolor1, bgcolor2) = color::bgcolor_for(opt.bgcolors, opt.colors);
    let style_options = StyleOptions {
        imageheight,
        bgcolor1,
        bgcolor2,
    };

    svg::write_prelude(&mut svg, &style_options, &opt)?;

    // Used when picking color parameters at random, when no option determines how to pick these
    // parameters. We instanciate it here because it may be called once for each iteration in the
    // frames loop.
    let mut thread_rng = rand::thread_rng();

    // structs to reuse accross loops to avoid allocations
    let mut cache_g = Event::Start({ BytesStart::owned_name("g") });
    let mut cache_a = Event::Start({ BytesStart::owned_name("a") });
    let mut cache_rect = Event::Empty(BytesStart::owned_name("rect"));
    let cache_g_end = Event::End(BytesEnd::borrowed(b"g"));
    let cache_a_end = Event::End(BytesEnd::borrowed(b"a"));

    // create frames container
    if let Event::Start(ref mut g) = cache_g {
        g.extend_attributes(std::iter::once(("id", "frames")));
    }
    svg.write_event(&cache_g)?;

    // draw frames
    let mut samples_txt_buffer = num_format::Buffer::default();
    for frame in frames {
        let x1 = XPAD + (frame.start_time as f64 * widthpertime) as usize;
        let x2 = XPAD + (frame.end_time as f64 * widthpertime) as usize;

        let (y1, y2) = match opt.direction {
            Direction::Straight => {
                let y1 = imageheight - opt.ypad2() - (frame.location.depth + 1) * opt.frame_height
                    + FRAMEPAD;
                let y2 = imageheight - opt.ypad2() - frame.location.depth * opt.frame_height;
                (y1, y2)
            }
            Direction::Inverted => {
                let y1 = opt.ypad1() + frame.location.depth * opt.frame_height;
                let y2 = opt.ypad1() + (frame.location.depth + 1) * opt.frame_height - FRAMEPAD;
                (y1, y2)
            }
        };
        let rect = Rectangle { x1, y1, x2, y2 };

        // The rounding here can differ from the Perl version when the fractional part is `0.5`.
        // The Perl version does `my $samples = sprintf "%.0f", ($etime - $stime) * $factor;`,
        // but this can format in strange ways as shown in these examples:
        //     `sprintf "%.0f", 1.5` produces "2"
        //     `sprintf "%.0f", 2.5` produces "2"
        //     `sprintf "%.0f", 3.5` produces "4"
        let samples = ((frame.end_time - frame.start_time) as f64 * opt.factor).round() as usize;

        // add thousands separators to `samples`
        let _ = samples_txt_buffer.write_formatted(&samples, &Locale::en);
        let samples_txt = samples_txt_buffer.as_str();

        let info = if frame.location.function.is_empty() && frame.location.depth == 0 {
            write!(buffer, "all ({} {}, 100%)", samples_txt, opt.count_name)
        } else {
            let pct = (100 * samples) as f64 / (timemax as f64 * opt.factor);
            let function = deannotate(&frame.location.function);
            match frame.delta {
                None => write!(
                    buffer,
                    "{} ({} {}, {:.2}%)",
                    function, samples_txt, opt.count_name, pct
                ),
                // Special case delta == 0 so we don't format percentage with a + sign.
                Some(delta) if delta == 0 => write!(
                    buffer,
                    "{} ({} {}, {:.2}%; 0.00%)",
                    function, samples_txt, opt.count_name, pct,
                ),
                Some(mut delta) => {
                    if opt.negate_differentials {
                        delta = -delta;
                    }
                    let delta_pct = (100 * delta) as f64 / (timemax as f64 * opt.factor);
                    write!(
                        buffer,
                        "{} ({} {}, {:.2}%; {:+.2}%)",
                        function, samples_txt, opt.count_name, pct, delta_pct
                    )
                }
            }
        };

        let frame_attributes = opt
            .func_frameattrs
            .frameattrs_for_func(frame.location.function);

        let mut has_href = false;
        let mut title = &buffer[info];
        if let Some(frame_attributes) = frame_attributes {
            if frame_attributes.attrs.contains_key("xlink:href") {
                write_container_attributes(&mut cache_a, &frame_attributes);
                svg.write_event(&cache_a)?;
                has_href = true;
            } else {
                write_container_attributes(&mut cache_g, &frame_attributes);
                svg.write_event(&cache_g)?;
            }
            if let Some(ref t) = frame_attributes.title {
                title = t.as_str();
            }
        } else if let Event::Start(ref mut c) = cache_g {
            c.clear_attributes();
            svg.write_event(&cache_g)?;
        }

        svg.write_event(Event::Start(BytesStart::borrowed_name(b"title")))?;
        svg.write_event(Event::Text(BytesText::from_plain_str(title)))?;
        svg.write_event(Event::End(BytesEnd::borrowed(b"title")))?;

        // select the color of the rectangle
        let color = if frame.location.function == "--" {
            color::VDGREY
        } else if frame.location.function == "-" {
            color::DGREY
        } else if let Some(mut delta) = frame.delta {
            if opt.negate_differentials {
                delta = -delta;
            }
            color::color_scale(delta, delta_max)
        } else if let Some(ref mut palette_map) = opt.palette_map {
            let colors = opt.colors;
            let hash = opt.hash;
            palette_map.find_color_for(&frame.location.function, |name| {
                color::color(colors, hash, name, &mut thread_rng)
            })
        } else {
            color::color(
                opt.colors,
                opt.hash,
                frame.location.function,
                &mut thread_rng,
            )
        };
        filled_rectangle(&mut svg, &mut buffer, &rect, color, &mut cache_rect)?;

        let fitchars =
            (rect.width() as f64 / (opt.font_size as f64 * opt.font_width)).trunc() as usize;
        let text: svg::TextArgument<'_> = if fitchars >= 3 {
            // room for one char plus two dots
            let f = deannotate(&frame.location.function);

            // TODO: use Unicode grapheme clusters instead
            if f.len() < fitchars {
                // no need to truncate
                f.into()
            } else {
                // need to truncate :'(
                use std::fmt::Write;
                let mut w = buffer.writer();
                for c in f.chars().take(fitchars - 2) {
                    w.write_char(c).expect("writing to buffer shouldn't fail");
                }
                w.write_str("..").expect("writing to buffer shouldn't fail");
                w.finish().into()
            }
        } else {
            // don't show the function name
            "".into()
        };

        // write the text
        svg::write_str(
            &mut svg,
            &mut buffer,
            svg::TextItem {
                x: rect.x1 as f64 + 3.0,
                y: 3.0 + (rect.y1 + rect.y2) as f64 / 2.0,
                text,
                extra: None,
            },
        )?;

        buffer.clear();
        if has_href {
            svg.write_event(&cache_a_end)?;
        } else {
            svg.write_event(&cache_g_end)?;
        }
    }

    svg.write_event(&cache_g_end)?;
    svg.write_event(Event::End(BytesEnd::borrowed(b"svg")))?;
    svg.write_event(Event::Eof)?;

    Ok(())
}

/// Writes atributes to the container, container could be g or a
fn write_container_attributes(event: &mut Event<'_>, frame_attributes: &FrameAttrs) {
    if let Event::Start(ref mut c) = event {
        c.clear_attributes();
        c.extend_attributes(
            frame_attributes
                .attrs
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str())),
        );
    } else {
        unreachable!("cache wrapper was of wrong type: {:?}", event);
    }
}

/// Produce a flame graph from a reader that contains a sequence of folded stack lines.
///
/// See [`from_sorted_lines`] for the expected format of each line.
///
/// The resulting flame graph will be written out to `writer` in SVG format.
pub fn from_reader<R, W>(opt: &mut Options<'_>, reader: R, writer: W) -> quick_xml::Result<()>
where
    R: Read,
    W: Write,
{
    from_readers(opt, iter::once(reader), writer)
}

/// Produce a flame graph from a set of readers that contain folded stack lines.
///
/// See [`from_sorted_lines`] for the expected format of each line.
///
/// The resulting flame graph will be written out to `writer` in SVG format.
pub fn from_readers<R, W>(opt: &mut Options<'_>, readers: R, writer: W) -> quick_xml::Result<()>
where
    R: IntoIterator,
    R::Item: Read,
    W: Write,
{
    let mut input = String::new();
    for mut reader in readers {
        reader
            .read_to_string(&mut input)
            .map_err(quick_xml::Error::Io)?;
    }
    from_lines(opt, input.lines(), writer)
}

/// Produce a flame graph from files that contain folded stack lines
/// and write the result to provided `writer`.
///
/// If files is empty, STDIN will be used as input.
pub fn from_files<W: Write>(
    opt: &mut Options<'_>,
    files: &[PathBuf],
    writer: W,
) -> quick_xml::Result<()> {
    if files.is_empty() || files.len() == 1 && files[0].to_str() == Some("-") {
        let stdin = io::stdin();
        let r = BufReader::with_capacity(128 * 1024, stdin.lock());
        from_reader(opt, r, writer)
    } else if files.len() == 1 {
        let r = File::open(&files[0]).map_err(quick_xml::Error::Io)?;
        from_reader(opt, r, writer)
    } else {
        let stdin = io::stdin();
        let mut stdin_added = false;
        let mut readers: Vec<Box<dyn Read>> = Vec::with_capacity(files.len());
        for infile in files.iter() {
            if infile.to_str() == Some("-") {
                if !stdin_added {
                    let r = BufReader::with_capacity(128 * 1024, stdin.lock());
                    readers.push(Box::new(r));
                    stdin_added = true;
                }
            } else {
                let r = File::open(infile).map_err(quick_xml::Error::Io)?;
                readers.push(Box::new(r));
            }
        }

        from_readers(opt, readers, writer)
    }
}

fn deannotate(f: &str) -> &str {
    if f.ends_with(']') {
        if let Some(ai) = f.rfind("_[") {
            if f[ai..].len() == 4 && "kwij".contains(&f[ai + 2..ai + 3]) {
                return &f[..ai];
            }
        }
    }
    f
}

fn filled_rectangle<W: Write>(
    svg: &mut Writer<W>,
    buffer: &mut StrStack,
    rect: &Rectangle,
    color: Color,
    cache_rect: &mut Event<'_>,
) -> quick_xml::Result<usize> {
    let x = write_usize(buffer, rect.x1);
    let y = write_usize(buffer, rect.y1);
    let width = write_usize(buffer, rect.width());
    let height = write_usize(buffer, rect.height());
    let color = write!(buffer, "rgb({},{},{})", color.r, color.g, color.b);

    if let Event::Empty(bytes_start) = cache_rect {
        // clear the state
        bytes_start.clear_attributes();
        bytes_start.extend_attributes(args!(
            "x" => &buffer[x],
            "y" => &buffer[y],
            "width" => &buffer[width],
            "height" => &buffer[height],
            "fill" => &buffer[color]
        ));
    } else {
        unreachable!("cache wrapper was of wrong type: {:?}", cache_rect);
    }
    svg.write_event(&cache_rect)
}

fn write_usize(buffer: &mut StrStack, value: usize) -> usize {
    let mut writer = buffer.writer();
    // OK to unwrap here because this `fmt::Write` implementation never returns an error.
    itoa::fmt(&mut writer, value).unwrap();
    writer.finish()
}
