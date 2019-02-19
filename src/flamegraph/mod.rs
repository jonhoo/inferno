mod attrs;

pub use attrs::FuncFrameAttrsMap;

use pretty_toa::ThousandsSep;
use quick_xml::{
    events::{BytesEnd, BytesStart, BytesText, Event},
    Writer,
};
use std::io;
use std::io::prelude::*;
use str_stack::StrStack;

mod color;
mod merge;
mod svg;
pub use color::Palette;
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

#[derive(Debug, Default)]
pub struct Options {
    pub colors: color::Palette,
    pub hash: bool,
    pub consistent_palette: bool,
    pub func_frameattrs: FuncFrameAttrsMap,
    pub direction: Direction,
    pub title: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Direction {
    Straight,
    Inverted,
}

macro_rules! args {
    ($($key:expr => $value:expr),*) => {{
        [$(($key, $value),)*].into_iter().map(|(k, v): &(&str, &str)| (*k, *v))
    }};
}

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

    let (bgcolor1, bgcolor2) = color::bgcolor_for(opt.colors);

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

    // draw frames
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

        let samples = frame.end_time - frame.start_time;
        let samples_txt = samples.thousands_sep();

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

        let mut title = &buffer[info];
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
        if let Some(attrs) = opt
            .func_frameattrs
            .frameattrs_for_func(frame.location.function)
        {
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

        svg.write_event(Event::Start({
            let mut g = BytesStart::borrowed_name(b"g").with_attributes(args!(
                "class" => class
            ));
            if let Some(style) = style {
                g.extend_attributes(std::iter::once(("style", style)));
            }
            g.extend_attributes(args!(
                "onmouseover" => onmouseover,
                "onmouseout" => onmouseout,
                "onclick" => onclick
            ));
            if let Some(extra) = g_extra {
                g.extend_attributes(extra.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            }
            g
        }))?;

        svg.write_event(Event::Start(BytesStart::borrowed_name(b"title")))?;
        svg.write_event(Event::Text(BytesText::from_plain_str(title)))?;
        svg.write_event(Event::End(BytesEnd::borrowed(b"title")))?;

        if let Some(href) = href {
            svg.write_event(Event::Start({
                let mut a = BytesStart::borrowed_name(b"a").with_attributes(args!(
                    "xlink:href" => href,
                    "target" => target
                ));
                if let Some(extra) = a_extra {
                    a.extend_attributes(extra.iter().map(|(k, v)| (k.as_str(), v.as_str())));
                }
                a
            }))?;
        }

        if frame.location.function == "--" {
            filled_rectangle(&mut svg, &mut buffer, x1, x2, y1, y2, color::VDGREY)?;
        } else if frame.location.function == "-" {
            filled_rectangle(&mut svg, &mut buffer, x1, x2, y1, y2, color::DGREY)?;
        } else if let Some(ref mut palette_map) = palette_map {
            let color = palette_map.find_color_for(&frame.location.function, |name| {
                color::color(opt.colors, opt.hash, name, &mut thread_rng)
            });
            filled_rectangle(&mut svg, &mut buffer, x1, x2, y1, y2, color)?;
        } else {
            let color = color::color(
                opt.colors,
                opt.hash,
                frame.location.function,
                &mut thread_rng,
            );
            filled_rectangle(&mut svg, &mut buffer, x1, x2, y1, y2, color)?;
        };

        let fitchars = ((x2 - x1) as f64 / (FONTSIZE as f64 * FONTWIDTH)).trunc() as usize;
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

        svg::write_str(
            &mut svg,
            &mut buffer,
            svg::TextItem {
                color: "rgb(0, 0, 0)",
                size: FONTSIZE,
                x: x1 as f64 + 3.0,
                y: 3.0 + (y1 + y2) as f64 / 2.0,
                text,
                location: None,
                extra: None,
            },
        )?;

        buffer.clear();
        if href.is_some() {
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
    x1: usize,
    x2: usize,
    y1: usize,
    y2: usize,
    color: (u8, u8, u8),
) -> quick_xml::Result<usize> {
    let x = write!(buffer, "{}", x1);
    let y = write!(buffer, "{}", y1);
    let width = write!(buffer, "{}", x2 - x1);
    let height = write!(buffer, "{}", y2 - y1);
    let color = write!(buffer, "rgb({},{},{})", color.0, color.1, color.2);

    svg.write_event(Event::Empty(
        BytesStart::borrowed_name(b"rect").with_attributes(args!(
                "x" => &buffer[x],
                "y" => &buffer[y],
                "width" => &buffer[width],
                "height" => &buffer[height],
                "fill" => &buffer[color]
        )),
    ))
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Straight
    }
}
