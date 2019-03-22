use super::{Direction, Options};
use quick_xml::{
    events::{BytesEnd, BytesStart, BytesText, Event},
    Writer,
};
use std::borrow::Cow;
use std::cell::RefCell;
use std::io::prelude::*;
use std::iter;
use str_stack::StrStack;

pub(super) enum TextArgument<'a> {
    String(Cow<'a, str>),
    FromBuffer(usize),
}

impl<'a> From<&'a str> for TextArgument<'a> {
    fn from(s: &'a str) -> Self {
        TextArgument::String(Cow::from(s))
    }
}

impl<'a> From<String> for TextArgument<'a> {
    fn from(s: String) -> Self {
        TextArgument::String(Cow::from(s))
    }
}

impl<'a> From<usize> for TextArgument<'a> {
    fn from(i: usize) -> Self {
        TextArgument::FromBuffer(i)
    }
}

pub(super) struct TextItem<'a, I> {
    pub(super) color: &'a str,
    pub(super) size: usize,
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) text: TextArgument<'a>,
    pub(super) location: Option<&'a str>,
    pub(super) extra: I,
}

pub(super) struct StyleOptions<'a> {
    pub(super) imageheight: usize,
    pub(super) bgcolor1: Cow<'a, str>,
    pub(super) bgcolor2: Cow<'a, str>,
}

pub fn write_header<W>(
    svg: &mut Writer<W>,
    imageheight: usize,
    opt: &Options,
) -> quick_xml::Result<()>
where
    W: Write,
{
    svg.write(br#"<?xml version="1.0" standalone="no"?>"#)?;
    svg.write(br#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">"#)?;
    svg.write_event(Event::Start(
        BytesStart::borrowed_name(b"svg").with_attributes(vec![
            ("version", "1.1"),
            ("width", &*format!("{}", opt.image_width)),
            ("height", &*format!("{}", imageheight)),
            ("onload", "init(evt)"),
            (
                "viewBox",
                &*format!("0 0 {} {}", opt.image_width, imageheight),
            ),
            ("xmlns", "http://www.w3.org/2000/svg"),
            ("xmlns:xlink", "http://www.w3.org/1999/xlink"),
        ]),
    ))?;
    svg.write_event(Event::Comment(BytesText::from_plain_str(
        "Flame graph stack visualization. \
         See https://github.com/brendangregg/FlameGraph for latest version, \
         and http://www.brendangregg.com/flamegraphs.html for examples.",
    )))?;
    svg.write_event(Event::Comment(BytesText::from_plain_str(
        format!("NOTES: {}", opt.notes).as_str(),
    )))?;
    Ok(())
}

pub(super) fn write_prelude<'a, W>(
    svg: &mut Writer<W>,
    style_options: &StyleOptions<'a>,
    opt: &Options,
) -> quick_xml::Result<()>
where
    W: Write,
{
    svg.write_event(Event::Start(BytesStart::borrowed_name(b"defs")))?;
    svg.write_event(Event::Start(BytesStart::borrowed(
        br#"linearGradient id="background" y1="0" y2="1" x1="0" x2="0""#,
        "linearGradient".len(),
    )))?;
    svg.write_event(Event::Empty(
        BytesStart::borrowed_name(b"stop").with_attributes(
            iter::once(("stop-color", &*style_options.bgcolor1))
                .chain(iter::once(("offset", "5%"))),
        ),
    ))?;
    svg.write_event(Event::Empty(
        BytesStart::borrowed_name(b"stop").with_attributes(
            iter::once(("stop-color", &*style_options.bgcolor2))
                .chain(iter::once(("offset", "95%"))),
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
var nametype = {};
var fontsize = {};
var fontwidth = {};
var xpad = {};
var inverted = {};
var searchcolor = '{}';",
        enquote('\'', &opt.name_type),
        opt.font_size,
        opt.font_width,
        super::XPAD,
        opt.direction == Direction::Inverted,
        opt.search_color
    ))))?;
    if !opt.no_javascript {
        svg.write_event(Event::CData(BytesText::from_escaped_str(include_str!(
            "flamegraph.js"
        ))))?;
    }
    svg.write_event(Event::End(BytesEnd::borrowed(b"script")))?;

    svg.write_event(Event::Empty(
        BytesStart::borrowed_name(b"rect").with_attributes(vec![
            ("x", "0"),
            ("y", "0"),
            ("width", &*format!("{}", opt.image_width)),
            ("height", &*format!("{}", style_options.imageheight)),
            ("fill", "url(#background)"),
        ]),
    ))?;

    // We don't care too much about allocating just for the prelude
    let mut buf = StrStack::new();
    write_str(
        svg,
        &mut buf,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: opt.font_size + 5,
            x: (opt.image_width / 2) as f64,
            y: (opt.font_size * 2) as f64,
            text: (&*opt.title).into(),
            location: Some("middle"),
            extra: None,
        },
        &opt.font_type,
    )?;

    if let Some(ref subtitle) = opt.subtitle {
        write_str(
            svg,
            &mut buf,
            TextItem {
                color: "rgb(160, 160, 160)",
                size: opt.font_size,
                x: (opt.image_width / 2) as f64,
                y: (opt.font_size * 4) as f64,
                text: (&**subtitle).into(),
                location: Some("middle"),
                extra: None,
            },
            &opt.font_type,
        )?;
    }

    write_str(
        svg,
        &mut buf,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: opt.font_size,
            x: super::XPAD as f64,
            y: (style_options.imageheight - (opt.ypad2() / 2)) as f64,
            text: " ".into(),
            location: None,
            extra: iter::once(("id", "details")),
        },
        &opt.font_type,
    )?;

    write_str(
        svg,
        &mut buf,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: opt.font_size,
            x: super::XPAD as f64,
            y: (opt.font_size * 2) as f64,
            text: "Reset Zoom".into(),
            location: None,
            extra: vec![
                ("id", "unzoom"),
                ("onclick", "unzoom()"),
                ("style", "opacity:0.0;cursor:pointer"),
            ],
        },
        &opt.font_type,
    )?;

    write_str(
        svg,
        &mut buf,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: opt.font_size,
            x: (opt.image_width - super::XPAD - 100) as f64,
            y: (opt.font_size * 2) as f64,
            text: "Search".into(),
            location: None,
            extra: vec![
                ("id", "search"),
                ("onmouseover", "searchover()"),
                ("onmouseout", "searchout()"),
                ("onclick", "search_prompt()"),
                ("style", "opacity:0.1;cursor:pointer"),
            ],
        },
        &opt.font_type,
    )?;

    write_str(
        svg,
        &mut buf,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: opt.font_size,
            x: (opt.image_width - super::XPAD - 100) as f64,
            y: (style_options.imageheight - (opt.ypad2() / 2)) as f64,
            text: " ".into(),
            location: None,
            extra: iter::once(("id", "matched")),
        },
        &opt.font_type,
    )?;

    Ok(())
}

pub(super) fn write_str<'a, W, I>(
    svg: &mut Writer<W>,
    buf: &mut StrStack,
    item: TextItem<'a, I>,
    font_type: &str,
) -> quick_xml::Result<usize>
where
    W: Write,
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let x = write!(buf, "{:.2}", item.x);
    let y = write!(buf, "{:.2}", item.y);
    let fs = write!(buf, "{}", item.size);
    let TextItem {
        text,
        extra,
        location,
        color,
        ..
    } = item;

    thread_local! {
        // reuse for all text elements to avoid allocations
        static TEXT: RefCell<Event<'static>> = RefCell::new(Event::Start(BytesStart::owned_name("text")))
    };
    TEXT.with(|start_event| {
        if let Event::Start(ref mut text) = *start_event.borrow_mut() {
            text.clear_attributes();
            text.extend_attributes(extra);
            text.extend_attributes(args!(
                "text-anchor" => location.unwrap_or("left"),
                "x" => &buf[x],
                "y" => &buf[y],
                "font-size" => &buf[fs],
                "font-family" => font_type,
                "fill" => color
            ));
        } else {
            unreachable!("cache wrapper was of wrong type: {:?}", start_event);
        }

        svg.write_event(&*start_event.borrow())
    })?;
    let s = match text {
        TextArgument::String(ref s) => &*s,
        TextArgument::FromBuffer(i) => &buf[i],
    };
    svg.write_event(Event::Text(BytesText::from_plain_str(s)))?;
    svg.write_event(Event::End(BytesEnd::borrowed(b"text")))
}

// Imported from the `enquote` crate @ 1.0.3.
// It's "unlicense" licensed, so that's fine.
fn enquote(quote: char, s: &str) -> String {
    // escapes any `quote` in `s`
    let escaped = s
        .chars()
        .map(|c| match c {
            // escapes the character if it's the quote
            _ if c == quote => format!("\\{}", quote),
            // escapes backslashes
            '\\' => "\\\\".into(),
            // no escape required
            _ => c.to_string(),
        })
        .collect::<String>();

    // enquotes escaped string
    quote.to_string() + &escaped + &quote.to_string()
}
