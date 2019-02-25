mod attrs;
mod color;
mod merge;
mod svg;

pub use attrs::FuncFrameAttrsMap;
pub use color::BackgroundColor;
pub use color::Palette;

use num_format::Locale;
use quick_xml::{
    events::{BytesEnd, BytesStart, BytesText, Event},
    Writer,
};
use std::io;
use std::io::prelude::*;
use str_stack::StrStack;
use svg::StyleOptions;

const IMAGEWIDTH: usize = 1200; // max width, pixels
const FRAMEHEIGHT: usize = 16; // max height is dynamic
const FONTSIZE: usize = 12; // base text size
const FONTWIDTH: f64 = 0.59; // avg width relative to FONTSIZE
const MINWIDTH: f64 = 0.1; // min function width, pixels
const YPAD1: usize = FONTSIZE * 3; // pad top, include title
const YPAD2: usize = FONTSIZE * 2 + 10; // pad bottom, include labels
const XPAD: usize = 10; // pad lefm and right
const FRAMEPAD: usize = 1; // vertical padding for frames
const PALETTE_FILE: &str = "palette.map";

/// Configure the flame graph.
#[derive(Debug, Default)]
pub struct Options {
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
    /// flame graph is generated, and the (random) color chosen for each function will be written
    /// into it. On subsequent invocations, functions that already have a colored registered in
    /// that file will be given the stored color rather than be assigned a new one. New functions
    /// will have their colors persisted for future runs.
    ///
    /// This feature was first implemented [by Shawn
    /// Sterling](https://github.com/brendangregg/FlameGraph/pull/25).
    pub consistent_palette: bool,

    /// Assign extra attributes to particular functions.
    ///
    /// In particular, if a function appears in the given map, it will have extra attributes set in
    /// the resulting SVG based on its value in the map.
    pub func_frameattrs: FuncFrameAttrsMap,

    /// Whether to plot a plot that grows top-to-bottom or bottom-up (the default).
    pub direction: Direction,

    /// The title for the flame chart.
    pub title: String,
}

/// The direction the plot should grow.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Direction {
    /// Stacks grow from the bottom to the top.
    ///
    /// `main` will be at the bottom.
    Straight,

    /// Stacks grow from the top to the bottom.
    ///
    /// `main` will be at the top.
    Inverted,
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Straight
    }
}

macro_rules! args {
    ($($key:expr => $value:expr),*) => {{
        [$(($key, $value),)*].into_iter().map(|(k, v): &(&str, &str)| (*k, *v))
    }};
}

struct FrameAttributes<'a> {
    title: &'a str,
    class: &'a str,
    onmouseover: &'a str,
    onmouseout: &'a str,
    onclick: &'a str,
    style: Option<&'a str>,
    g_extra: Option<&'a Vec<(String, String)>>,
    href: Option<&'a str>,
    target: &'a str,
    a_extra: Option<&'a Vec<(String, String)>>,
}

fn override_or_add_attributes<'a>(
    title: &'a str,
    attributes: Option<&'a attrs::FrameAttrs>,
) -> FrameAttributes<'a> {
    let mut title = title;
    let mut class = "func_g";
    let mut onmouseover = "s(this)";
    let mut onmouseout = "c()";
    let mut onclick = "zoom(this)";
    let mut style = None;
    let mut g_extra = None;
    let mut href = None;
    let mut target = "_top";
    let mut a_extra = None;

    // Handle any overridden or extra attributes.
    if let Some(attrs) = attributes {
        if let Some(ref c) = attrs.g.class {
            class = c.as_str();
        }
        if let Some(ref c) = attrs.g.style {
            style = Some(c.as_str());
        }
        if let Some(ref o) = attrs.g.onmouseover {
            onmouseover = o.as_str();
        }
        if let Some(ref o) = attrs.g.onmouseout {
            onmouseout = o.as_str();
        }
        if let Some(ref o) = attrs.g.onclick {
            onclick = o.as_str();
        }
        if let Some(ref t) = attrs.title {
            title = t.as_str();
        }
        g_extra = Some(&attrs.g.extra);
        if let Some(ref h) = attrs.a.href {
            href = Some(h.as_str());
        }
        if let Some(ref t) = attrs.a.target {
            target = t.as_str();
        }
        a_extra = Some(&attrs.a.extra);
    }

    FrameAttributes {
        title,
        class,
        onmouseover,
        onmouseout,
        onclick,
        style,
        g_extra,
        href,
        target,
        a_extra,
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

/// Produce a flame graph from a sorted iterator over folded stack lines.
///
/// This function expects each folded stack to contain the following whitespace-separated fields:
///
///  - A semicolon-separated list of frame names (e.g., `main;foo;bar;baz`).
///  - A sample count for the given stack.
///
/// The resulting flame graph will be written out to `writer` in SVG format.
pub fn from_sorted_lines<'a, I, W>(opt: Options, lines: I, writer: W) -> quick_xml::Result<()>
where
    I: IntoIterator<Item = &'a str>,
    W: Write,
{
    let mut palette_map = if opt.consistent_palette {
        Some(color::PaletteMap::load(PALETTE_FILE)?)
    } else {
        None
    };

    let (bgcolor1, bgcolor2) = color::bgcolor_for(opt.bgcolors, opt.colors);

    let mut buffer = StrStack::new();
    let (mut frames, time, ignored) = merge::frames(lines);
    if ignored != 0 {
        warn!("Ignored {} lines with invalid format", ignored);
    }

    // let's start writing the svg!
    let mut svg = Writer::new(writer);
    if time == 0 {
        error!("No stack counts found");
        // emit an error message SVG, for tools automating flamegraph use
        let imageheight = FONTSIZE * 5;
        svg::write_header(&mut svg, imageheight)?;
        svg::write_str(
            &mut svg,
            &mut buffer,
            svg::TextItem {
                color: "black",
                size: FONTSIZE + 2,
                x: (IMAGEWIDTH / 2) as f64,
                y: (FONTSIZE * 2) as f64,
                text: "ERROR: No valid input provided to flamegraph".into(),
                location: Some("middle"),
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
    let widthpertime = (IMAGEWIDTH - 2 * XPAD) as f64 / timemax as f64;
    let minwidth_time = MINWIDTH / widthpertime;

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
    let imageheight = ((depthmax + 1) * FRAMEHEIGHT) + YPAD1 + YPAD2;
    svg::write_header(&mut svg, imageheight)?;

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

    // draw frames
    let mut samples_txt_buffer = num_format::Buffer::default();
    for frame in frames {
        let x1 = XPAD + (frame.start_time as f64 * widthpertime) as usize;
        let x2 = XPAD + (frame.end_time as f64 * widthpertime) as usize;
        let (y1, y2) = match opt.direction {
            Direction::Straight => {
                let y1 = imageheight - YPAD2 - (frame.location.depth + 1) * FRAMEHEIGHT + FRAMEPAD;
                let y2 = imageheight - YPAD2 - frame.location.depth * FRAMEHEIGHT;
                (y1, y2)
            }
            Direction::Inverted => {
                let y1 = YPAD1 + frame.location.depth * FRAMEHEIGHT;
                let y2 = YPAD1 + (frame.location.depth + 1) * FRAMEHEIGHT - FRAMEPAD;
                (y1, y2)
            }
        };
        let rect = Rectangle { x1, y1, x2, y2 };

        let samples = frame.end_time - frame.start_time;

        // add thousands separators to `samples`
        let _ = samples_txt_buffer.write_formatted(&samples, &Locale::en);
        let samples_txt = samples_txt_buffer.as_str();

        let info = if frame.location.function.is_empty() && frame.location.depth == 0 {
            write!(buffer, "all ({} samples, 100%)", samples_txt)
        } else {
            let pct = (100 * samples) as f64 / timemax as f64;

            // strip any annotation
            write!(
                buffer,
                "{} ({} samples, {:.2}%)",
                deannotate(&frame.location.function),
                samples_txt,
                pct
            )
        };

        let frame_attributes = opt
            .func_frameattrs
            .frameattrs_for_func(frame.location.function);
        let frame_attributes = override_or_add_attributes(&buffer[info], frame_attributes);
        let href_is_some = frame_attributes.href.is_some();

        if let Event::Start(ref mut g) = cache_g {
            // clear the BytesStart
            g.clear_attributes();

            g.extend_attributes(args!(
                "class" => frame_attributes.class,
                "onmouseover" => frame_attributes.onmouseover,
                "onmouseout" => frame_attributes.onmouseout,
                "onclick" => frame_attributes.onclick
            ));

            // add optional attributes
            if let Some(style) = frame_attributes.style {
                g.extend_attributes(std::iter::once(("style", style)));
            }
            if let Some(extra) = frame_attributes.g_extra {
                g.extend_attributes(extra.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            }
        } else {
            unreachable!("cache wrapper was of wrong type: {:?}", cache_g);
        }

        svg.write_event(&cache_g)?;

        svg.write_event(Event::Start(BytesStart::borrowed_name(b"title")))?;
        svg.write_event(Event::Text(BytesText::from_plain_str(
            frame_attributes.title,
        )))?;
        svg.write_event(Event::End(BytesEnd::borrowed(b"title")))?;

        if let Some(href) = frame_attributes.href {
            if let Event::Start(ref mut a) = cache_a {
                // clear the BytesStart
                a.clear_attributes();

                a.extend_attributes(args!(
                    "xlink:href" => href,
                    "target" => frame_attributes.target
                ));
                if let Some(extra) = frame_attributes.a_extra {
                    a.extend_attributes(extra.iter().map(|(k, v)| (k.as_str(), v.as_str())));
                }
            } else {
                unreachable!("cache wrapper was of wrong type: {:?}", cache_a);
            }

            svg.write_event(&cache_a)?;
        }

        // select the color of the rectangle
        let color = if frame.location.function == "--" {
            color::VDGREY
        } else if frame.location.function == "-" {
            color::DGREY
        } else if let Some(ref mut palette_map) = palette_map {
            palette_map.find_color_for(&frame.location.function, |name| {
                color::color(opt.colors, opt.hash, name, &mut thread_rng)
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

        let fitchars = (rect.width() as f64 / (FONTSIZE as f64 * FONTWIDTH)).trunc() as usize;
        let text: svg::TextArgument = if fitchars >= 3 {
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
                color: "rgb(0, 0, 0)",
                size: FONTSIZE,
                x: rect.x1 as f64 + 3.0,
                y: 3.0 + (rect.y1 + rect.y2) as f64 / 2.0,
                text,
                location: None,
                extra: None,
            },
        )?;

        buffer.clear();
        if href_is_some {
            svg.write_event(Event::End(BytesEnd::borrowed(b"a")))?;
        }
        svg.write_event(Event::End(BytesEnd::borrowed(b"g")))?;
    }

    svg.write_event(Event::End(BytesEnd::borrowed(b"svg")))?;
    svg.write_event(Event::Eof)?;

    if let Some(palette_map) = palette_map {
        palette_map
            .save(PALETTE_FILE)
            .map_err(quick_xml::Error::Io)?;
    }

    Ok(())
}

/// Produce a flame graph from a reader that contains a sorted sequence of folded stack lines.
///
/// See [`from_sorted_lines`] for the expected format of each line.
///
/// The resulting flame graph will be written out to `writer` in SVG format.
pub fn from_reader<R, W>(opt: Options, mut reader: R, writer: W) -> quick_xml::Result<()>
where
    R: Read,
    W: Write,
{
    let mut input = String::new();
    reader
        .read_to_string(&mut input)
        .map_err(quick_xml::Error::Io)?;

    from_sorted_lines(opt, input.lines(), writer)
}

/// Produce a flame graph from a set of reader that contain folded stack lines.
///
/// This function sorts all the read lines before processing them.
/// See [`from_sorted_lines`] for the expected format of each line.
///
/// The resulting flame graph will be written out to `writer` in SVG format.
pub fn from_readers<R, W>(opt: Options, readers: R, writer: W) -> quick_xml::Result<()>
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

    let mut lines: Vec<&str> = input.lines().collect();
    lines.sort_unstable();
    from_sorted_lines(opt, lines, writer)
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
    color: (u8, u8, u8),
    cache_rect: &mut Event,
) -> quick_xml::Result<usize> {
    let x = write!(buffer, "{}", rect.x1);
    let y = write!(buffer, "{}", rect.y1);
    let width = write!(buffer, "{}", rect.width());
    let height = write!(buffer, "{}", rect.height());
    let color = write!(buffer, "rgb({},{},{})", color.0, color.1, color.2);

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
