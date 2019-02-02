use pretty_toa::ThousandsSep;
use quick_xml::{
    events::attributes::Attribute,
    events::{BytesEnd, BytesStart, BytesText, Event},
    Writer,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::io;
use std::io::prelude::*;
use std::iter;

#[derive(Debug, Default)]
pub struct Options {}

#[derive(Debug, PartialEq, Eq, Hash)]
struct Frame {
    function: String,
    depth: usize,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct TimedFrame {
    location: Frame,
    start_time: usize,
    end_time: usize,
}

fn flow<'a, LI, TI>(
    tmp: &mut HashMap<Frame, usize>,
    frames: &mut Vec<TimedFrame>,
    last: LI,
    this: TI,
    time: usize,
) where
    LI: IntoIterator<Item = &'a str>,
    TI: IntoIterator<Item = &'a str>,
{
    let mut this = this.into_iter().peekable();
    let mut last = last.into_iter().peekable();

    // remove common prefix
    let mut shared_depth = 0;
    while last.peek() == this.peek() {
        // they must both be None, so let's stop looping
        if last.peek().is_none() {
            break;
        }

        // move along prefix iterators
        let _ = last.next();
        let _ = this.next();
        shared_depth += 1;
    }

    // TODO: document this..

    for (i, func) in last.enumerate() {
        let key = Frame {
            function: func.to_string(),
            depth: shared_depth + i,
        };

        //eprintln!("at {} ending frame {:?}", time, key);
        let start_time = tmp.remove(&key).unwrap_or_else(|| {
            unreachable!("did not have start time for {:?}", key);
        });

        let key = TimedFrame {
            location: key,
            start_time,
            end_time: time,
        };
        frames.push(key);
    }

    for (i, func) in this.enumerate() {
        let key = Frame {
            function: func.to_string(),
            depth: shared_depth + i,
        };
        //eprintln!("stored tmp for time {}: {:?}", time, key);
        if let Some(start_time) = tmp.insert(key, time) {
            unreachable!("start time {} already registered for frame", start_time);
        }
    }
}

pub fn handle_file<R: BufRead, W: Write>(
    _opt: Options,
    mut reader: R,
    writer: W,
) -> quick_xml::Result<()> {
    let imagewidth = 1200; // max width, pixels
    let frameheight = 16; // max height is dynamic
    let fontsize = 12; // base text size
    let fontwidth = 0.59; // avg width relative to fontsize
    let minwidth = 0.1; // min function width, pixels
    let ypad1 = fontsize * 3; // pad top, include title
    let ypad2 = fontsize * 2 + 10; // pad bottom, include labels
    let xpad = 10; // pad lefm and right
    let framepad = 1; // vertical padding for frames
    let bgcolor1 = "#eeeeee";
    let bgcolor2 = "#eeeeb0";

    // TODO: technically need to pre-process lines (reverse stacks + sort in case of multi-file)
    // would also let us only operate on &str

    let mut time = 0;
    let mut ignored = 0;
    let mut last = String::new();
    let mut tmp = Default::default();
    let mut frames = Default::default();
    let mut line = String::new();
    loop {
        line.clear();

        if reader.read_line(&mut line).map_err(quick_xml::Error::Io)? == 0 {
            break;
        }

        let mut line = line.trim();
        if line.is_empty() {
            continue;
        }

        let nsamples = if let Some(samplesi) = line.rfind(' ') {
            let mut samples = &line[(samplesi + 1)..];
            // strip fractional part (if any);
            // foobar 1.klwdjlakdj
            if let Some(doti) = samples.find('.') {
                samples = &samples[..doti];
            }
            match samples.parse::<usize>() {
                Ok(nsamples) => {
                    // remove nsamples part we just parsed from line
                    line = line[..samplesi].trim_end();
                    // give out the sample count
                    nsamples
                }
                Err(_) => {
                    ignored += 1;
                    continue;
                }
            }
        } else {
            ignored += 1;
            continue;
        };

        if line.is_empty() {
            ignored += 1;
            continue;
        }
        let stack = line;

        // inject empty first-level stack frame to capture "all"
        let this = iter::once("").chain(stack.split(';'));
        if last.is_empty() {
            // need to special-case this, because otherwise iter("") + "".split(';') == ["", ""]
            //eprintln!("flow(_, {}, {})", stack, time);
            flow(&mut tmp, &mut frames, None, this, time);
        } else {
            //eprintln!("flow({}, {}, {})", last, stack, time);
            flow(
                &mut tmp,
                &mut frames,
                iter::once("").chain(last.split(';')),
                this,
                time,
            );
        }

        last = stack.to_string();
        time += nsamples;
    }
    if !last.is_empty() {
        //eprintln!("flow({}, _, {})", last, time);
        flow(
            &mut tmp,
            &mut frames,
            iter::once("").chain(last.split(';')),
            None,
            time,
        );
    }

    if ignored != 0 {
        eprintln!("Ignored {} lines with invalid format", ignored);
    }

    // set up the SVG file header
    let mut svg = Writer::new(writer);
    let write_header = |svg: &mut Writer<W>, imagewidth: usize, imageheight: usize| {
        svg.write(br#"<?xml version="1.0" standalone="no"?>"#)?;
        svg.write(br#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">"#)?;
        svg.write_event(Event::Start(
            BytesStart::borrowed_name(b"svg").with_attributes(vec![
                ("version", "1.1"),
                ("width", &*format!("{}", imagewidth)),
                ("height", &*format!("{}", imageheight)),
                ("onload", "init(evt)"),
                ("viewBox", &*format!("0 0 {} {}", imagewidth, imageheight)),
                ("xmlns", "http://www.w3.org/2000/svg"),
                ("xmlns:xlink", "http://www.w3.org/1999/xlink"),
            ]),
        ))?;
        svg.write_event(Event::Comment(BytesText::from_plain_str(
            "Flame graph stack visualization. \
             See https://github.com/brendangregg/FlameGraph for latest version, \
             and http://www.brendangregg.com/flamegraphs.html for examples.",
        )))?;
        Ok(())
    };

    if time == 0 {
        eprintln!("ERROR: No stack counts found");
        // emit an error message SVG, for tools automating flamegraph use
        let imageheight = fontsize * 5;
        write_header(&mut svg, imagewidth, imageheight)?;
        write_svg_str(
            &mut svg,
            TextItem {
                color: "black",
                size: fontsize + 2,
                x: (imagewidth / 2) as f64,
                y: (fontsize * 2) as f64,
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

    // TODO: --total
    let timemax = time;
    let widthpertime = (imagewidth - 2 * xpad) as f64 / timemax as f64;
    let minwidth_time = minwidth / widthpertime;

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

    let imageheight = ((depthmax + 1) * frameheight) + ypad1 + ypad2;
    write_header(&mut svg, imagewidth, imageheight)?;

    // draw canvas, and embed interactive JavaScript program
    svg.write_event(Event::Start(BytesStart::borrowed_name(b"defs")))?;
    svg.write_event(Event::Start(BytesStart::borrowed(
        br#"linearGradient id="background" y1="0" y2="1" x1="0" x2="0""#,
        "linearGradient".len(),
    )))?;
    svg.write_event(Event::Empty(
        BytesStart::borrowed_name(b"stop").with_attributes(
            iter::once(("stop-color", bgcolor1)).chain(iter::once(("offset", "5%"))),
        ),
    ))?;
    svg.write_event(Event::Empty(
        BytesStart::borrowed_name(b"stop").with_attributes(
            iter::once(("stop-color", bgcolor2)).chain(iter::once(("offset", "95%"))),
        ),
    ))?;
    svg.write_event(Event::End(BytesEnd::borrowed(b"linearGradient")))?;
    svg.write_event(Event::End(BytesEnd::borrowed(b"defs")))?;

    svg.write_event(Event::Start(
        BytesStart::borrowed_name(b"style").with_attributes(iter::once(("type", "text/css"))),
    ))?;
    svg.write_event(Event::Text(BytesText::from_plain_str(
        ".func_g:hover { stroke:black; stroke-width:0.5; cursor:pointer; }",
    )))?;
    svg.write_event(Event::End(BytesEnd::borrowed(b"style")))?;

    svg.write_event(Event::Start(
        BytesStart::borrowed_name(b"script")
            .with_attributes(iter::once(("type", "text/ecmascript"))),
    ))?;
    svg.write_event(Event::CData(BytesText::from_escaped_str(&format!(
        "\
var nametype = 'Function:';
var fontsize = {};
var fontwidth = {};
var xpad = {};
var inverted = false;
var searchcolor = 'rgb(230,0,230)';",
        fontsize, fontwidth, xpad,
    ))))?;
    svg.write_event(Event::CData(BytesText::from_escaped_str(include_str!(
        "flamegraph.js"
    ))))?;
    svg.write_event(Event::End(BytesEnd::borrowed(b"script")))?;

    svg.write_event(Event::Empty(
        BytesStart::borrowed_name(b"rect").with_attributes(vec![
            ("x", "0"),
            ("y", "0"),
            ("width", &*format!("{}", imagewidth)),
            ("height", &*format!("{}", imageheight)),
            ("fill", "url(#background)"),
        ]),
    ))?;

    write_svg_str(
        &mut svg,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: fontsize + 5,
            x: (imagewidth / 2) as f64,
            y: (fontsize * 2) as f64,
            text: "Flame Graph",
            location: Some("middle"),
            extra: None,
        },
    )?;

    write_svg_str(
        &mut svg,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: fontsize,
            x: xpad as f64,
            y: (imageheight - (ypad2 / 2)) as f64,
            text: " ",
            location: None,
            extra: iter::once(("id", "details")),
        },
    )?;

    write_svg_str(
        &mut svg,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: fontsize,
            x: xpad as f64,
            y: (fontsize * 2) as f64,
            text: "Reset Zoom",
            location: None,
            extra: vec![
                ("id", "unzoom"),
                ("onclick", "unzoom()"),
                ("style", "opacity:0.0;cursor:pointer"),
            ],
        },
    )?;

    write_svg_str(
        &mut svg,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: fontsize,
            x: (imagewidth - xpad - 100) as f64,
            y: (fontsize * 2) as f64,
            text: "Search",
            location: None,
            extra: vec![
                ("id", "search"),
                ("onmouseover", "searchover()"),
                ("onmouseout", "searchout()"),
                ("onclick", "search_prompt()"),
                ("style", "opacity:0.1;cursor:pointer"),
            ],
        },
    )?;

    write_svg_str(
        &mut svg,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: fontsize,
            x: (imagewidth - xpad - 100) as f64,
            y: (imageheight - (ypad2 / 2)) as f64,
            text: " ",
            location: None,
            extra: iter::once(("id", "matched")),
        },
    )?;

    // draw frames
    for frame in frames {
        let x1 = xpad + (frame.start_time as f64 * widthpertime) as usize;
        let x2 = xpad + (frame.end_time as f64 * widthpertime) as usize;
        let y1 = imageheight - ypad2 - (frame.location.depth + 1) * frameheight + framepad;
        let y2 = imageheight - ypad2 - frame.location.depth * frameheight;

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

        let fitchars = ((x2 - x1) as f64 / (fontsize as f64 * fontwidth)).trunc() as usize;
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

        write_svg_str(
            &mut svg,
            TextItem {
                color: "rgb(0, 0, 0)",
                size: fontsize,
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

struct TextItem<'a, I> {
    color: &'a str,
    size: usize,
    x: f64,
    y: f64,
    text: &'a str,
    location: Option<&'a str>,
    extra: I,
}

fn write_svg_str<'a, W, I>(svg: &mut Writer<W>, item: TextItem<'a, I>) -> quick_xml::Result<()>
where
    W: Write,
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut text = BytesStart::borrowed_name(b"text").with_attributes(item.extra);
    text.push_attribute(Attribute::from((
        "text-anchor",
        item.location.unwrap_or("left"),
    )));
    text.push_attribute(Attribute {
        key: b"x",
        value: Vec::from(format!("{:.2}", item.x)).into(),
    });
    text.push_attribute(Attribute {
        key: b"y",
        value: Vec::from(format!("{:.2}", item.y)).into(),
    });
    text.push_attribute(Attribute {
        key: b"font-size",
        value: Vec::from(item.size.to_string()).into(),
    });
    text.push_attribute(Attribute::from(("font-family", "Verdana")));
    text.push_attribute(Attribute::from(("fill", item.color)));
    svg.write_event(Event::Start(text))?;
    svg.write_event(Event::Text(BytesText::from_plain_str(item.text)))?;
    svg.write_event(Event::End(BytesEnd::borrowed(b"text")))?;
    Ok(())
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
