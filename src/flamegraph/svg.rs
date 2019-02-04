use quick_xml::{
    events::attributes::Attribute,
    events::{BytesEnd, BytesStart, BytesText, Event},
    Writer,
};
use std::borrow::Cow;
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

pub(super) fn write_header<W>(svg: &mut Writer<W>, imageheight: usize) -> quick_xml::Result<()>
where
    W: Write,
{
    svg.write(br#"<?xml version="1.0" standalone="no"?>"#)?;
    svg.write(br#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">"#)?;
    svg.write_event(Event::Start(
        BytesStart::borrowed_name(b"svg").with_attributes(vec![
            ("version", "1.1"),
            ("width", &*format!("{}", super::IMAGEWIDTH)),
            ("height", &*format!("{}", imageheight)),
            ("onload", "init(evt)"),
            (
                "viewBox",
                &*format!("0 0 {} {}", super::IMAGEWIDTH, imageheight),
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
    Ok(())
}

pub(super) fn write_prelude<W>(
    svg: &mut Writer<W>,
    imageheight: usize,
    bgcolor1: &str,
    bgcolor2: &str,
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
        super::FONTSIZE,
        super::FONTWIDTH,
        super::XPAD,
    ))))?;
    svg.write_event(Event::CData(BytesText::from_escaped_str(include_str!(
        "flamegraph.js"
    ))))?;
    svg.write_event(Event::End(BytesEnd::borrowed(b"script")))?;

    svg.write_event(Event::Empty(
        BytesStart::borrowed_name(b"rect").with_attributes(vec![
            ("x", "0"),
            ("y", "0"),
            ("width", &*format!("{}", super::IMAGEWIDTH)),
            ("height", &*format!("{}", imageheight)),
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
            size: super::FONTSIZE + 5,
            x: (super::IMAGEWIDTH / 2) as f64,
            y: (super::FONTSIZE * 2) as f64,
            text: "Flame Graph".into(),
            location: Some("middle"),
            extra: None,
        },
    )?;

    write_str(
        svg,
        &mut buf,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: super::FONTSIZE,
            x: super::XPAD as f64,
            y: (imageheight - (super::YPAD2 / 2)) as f64,
            text: " ".into(),
            location: None,
            extra: iter::once(("id", "details")),
        },
    )?;

    write_str(
        svg,
        &mut buf,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: super::FONTSIZE,
            x: super::XPAD as f64,
            y: (super::FONTSIZE * 2) as f64,
            text: "Reset Zoom".into(),
            location: None,
            extra: vec![
                ("id", "unzoom"),
                ("onclick", "unzoom()"),
                ("style", "opacity:0.0;cursor:pointer"),
            ],
        },
    )?;

    write_str(
        svg,
        &mut buf,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: super::FONTSIZE,
            x: (super::IMAGEWIDTH - super::XPAD - 100) as f64,
            y: (super::FONTSIZE * 2) as f64,
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
    )?;

    write_str(
        svg,
        &mut buf,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: super::FONTSIZE,
            x: (super::IMAGEWIDTH - super::XPAD - 100) as f64,
            y: (imageheight - (super::YPAD2 / 2)) as f64,
            text: " ".into(),
            location: None,
            extra: iter::once(("id", "matched")),
        },
    )?;

    Ok(())
}

pub(super) fn write_str<'a, W, I>(
    svg: &mut Writer<W>,
    buf: &mut StrStack,
    item: TextItem<'a, I>,
) -> quick_xml::Result<()>
where
    W: Write,
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let x = write!(buf, "{:.2}", item.x);
    let y = write!(buf, "{:.2}", item.y);
    let fs = write!(buf, "{}", item.size);
    let mut text = BytesStart::borrowed_name(b"text").with_attributes(item.extra);
    text.push_attribute(Attribute::from((
        "text-anchor",
        item.location.unwrap_or("left"),
    )));
    text.push_attribute(Attribute {
        key: b"x",
        value: Cow::from(buf[x].as_bytes()),
    });
    text.push_attribute(Attribute {
        key: b"y",
        value: Cow::from(buf[y].as_bytes()),
    });
    text.push_attribute(Attribute {
        key: b"font-size",
        value: Cow::from(buf[fs].as_bytes()),
    });
    text.push_attribute(Attribute::from(("font-family", "Verdana")));
    text.push_attribute(Attribute::from(("fill", item.color)));
    svg.write_event(Event::Start(text))?;
    let s = match item.text {
        TextArgument::String(ref s) => &*s,
        TextArgument::FromBuffer(i) => &buf[i],
    };
    svg.write_event(Event::Text(BytesText::from_plain_str(s)))?;
    svg.write_event(Event::End(BytesEnd::borrowed(b"text")))?;
    Ok(())
}
