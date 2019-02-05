use pretty_toa::ThousandsSep;
use quick_xml::{
    events::{BytesEnd, BytesStart, BytesText, Event},
    Writer,
};
use std::borrow::Cow;
use std::io;
use std::io::prelude::*;

mod merge;
mod svg;

const IMAGEWIDTH: usize = 1200; // max width, pixels
const FRAMEHEIGHT: usize = 16; // max height is dynamic
const FONTSIZE: usize = 12; // base text size
const FONTWIDTH: f64 = 0.59; // avg width relative to FONTSIZE
const MINWIDTH: f64 = 0.1; // min function width, pixels
const YPAD1: usize = FONTSIZE * 3; // pad top, include title
const YPAD2: usize = FONTSIZE * 2 + 10; // pad bottom, include labels
const XPAD: usize = 10; // pad lefm and right
const FRAMEPAD: usize = 1; // vertical padding for frames
const BGCOLOR1: &str = "#eeeeee";
const BGCOLOR2: &str = "#eeeeb0";

#[derive(Debug, Default)]
pub struct Options {}

pub fn from_sorted_lines<'a, I, W>(_opt: Options, lines: I, writer: W) -> quick_xml::Result<()>
where
    I: IntoIterator<Item = &'a str>,
    W: Write,
{
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
            svg::TextItem {
                color: "black",
                size: FONTSIZE + 2,
                x: (IMAGEWIDTH / 2) as f64,
                y: (FONTSIZE * 2) as f64,
                text: "ERROR: No valid input provided to flamegraph",
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
    svg::write_prelude(&mut svg, imageheight)?;

    // draw frames
    for frame in frames {
        let x1 = XPAD + (frame.start_time as f64 * widthpertime) as usize;
        let x2 = XPAD + (frame.end_time as f64 * widthpertime) as usize;
        let y1 = imageheight - YPAD2 - (frame.location.depth + 1) * FRAMEHEIGHT + FRAMEPAD;
        let y2 = imageheight - YPAD2 - frame.location.depth * FRAMEHEIGHT;

        let samples = frame.end_time - frame.start_time;
        let samples_txt = samples.thousands_sep();

        let info = if frame.location.function.is_empty() && frame.location.depth == 0 {
            format!("all ({} samples, 100%)", samples_txt)
        } else {
            let pct = (100 * samples) as f64 / timemax as f64;

            // strip any annotation
            format!(
                "{} ({} samples, {:.2}%)",
                deannotate(&frame.location.function),
                samples_txt,
                pct
            )
        };

        svg.write_event(Event::Start(
            BytesStart::borrowed_name(b"g").with_attributes(vec![
                ("class", "func_g"),
                ("onmouseover", "s(this)"),
                ("onmouseout", "c()"),
                ("onclick", "zoom(this)"),
            ]),
        ))?;
        svg.write_event(Event::Start(BytesStart::borrowed_name(b"title")))?;
        svg.write_event(Event::Text(BytesText::from_plain_str(&*info)))?;
        svg.write_event(Event::End(BytesEnd::borrowed(b"title")))?;

        let color = "rgb(242,10,32)";
        svg.write_event(Event::Empty(
            BytesStart::borrowed_name(b"rect").with_attributes(vec![
                ("x", &*format!("{}", x1)),
                ("y", &*format!("{}", y1)),
                ("width", &*format!("{}", x2 - x1)),
                ("height", &*format!("{}", y2 - y1)),
                ("fill", color),
            ]),
        ))?;

        let fitchars = ((x2 - x1) as f64 / (FONTSIZE as f64 * FONTWIDTH)).trunc() as usize;
        let text = if fitchars >= 3 {
            // room for one char plus two dots
            let f = deannotate(&frame.location.function);

            // TODO: use Unicode grapheme clusters instead
            if f.len() < fitchars {
                // no need to truncate
                Cow::from(f)
            } else {
                // need to truncate :'(
                let mut s = String::with_capacity(fitchars);
                s.extend(f.chars().take(fitchars - 2));
                s.push_str("..");
                Cow::from(s)
            }
        } else {
            // don't show the function name
            Cow::from("")
        };

        svg::write_str(
            &mut svg,
            svg::TextItem {
                color: "rgb(0, 0, 0)",
                size: FONTSIZE,
                x: x1 as f64 + 3.0,
                y: 3.0 + (y1 + y2) as f64 / 2.0,
                text: &*text,
                location: None,
                extra: None,
            },
        )?;

        svg.write_event(Event::End(BytesEnd::borrowed(b"g")))?;
    }

    svg.write_event(Event::End(BytesEnd::borrowed(b"svg")))?;
    svg.write_event(Event::Eof)?;
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
