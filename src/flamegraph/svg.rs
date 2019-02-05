use quick_xml::{
    events::attributes::Attribute,
    events::{BytesEnd, BytesStart, BytesText, Event},
    Writer,
};
use std::io::prelude::*;
use std::iter;

pub(super) struct TextItem<'a, I> {
    pub(super) color: &'a str,
    pub(super) size: usize,
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) text: &'a str,
    pub(super) location: Option<&'a str>,
    pub(super) extra: I,
}

pub(super) struct StyleOptions<'a> {
    imageheight: usize,
    bgcolor1: &'a str,
    bgcolor2: &'a str,
}

impl<'a> StyleOptions<'a> {
    pub(super) fn new(imageheight: usize, bgcolor1: &'a str, bgcolor2: &'a str) -> Self {
        StyleOptions {
            imageheight,
            bgcolor1,
            bgcolor2,
        }
    }
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

pub(super) fn write_prelude<'a, W>(
    svg: &mut Writer<W>,
    style_options: &StyleOptions<'a>,
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
            iter::once(("stop-color", style_options.bgcolor1)).chain(iter::once(("offset", "5%"))),
        ),
    ))?;
    svg.write_event(Event::Empty(
        BytesStart::borrowed_name(b"stop").with_attributes(
            iter::once(("stop-color", style_options.bgcolor2)).chain(iter::once(("offset", "95%"))),
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
            ("height", &*format!("{}", style_options.imageheight)),
            ("fill", "url(#background)"),
        ]),
    ))?;

    write_str(
        svg,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: super::FONTSIZE + 5,
            x: (super::IMAGEWIDTH / 2) as f64,
            y: (super::FONTSIZE * 2) as f64,
            text: "Flame Graph",
            location: Some("middle"),
            extra: None,
        },
    )?;

    write_str(
        svg,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: super::FONTSIZE,
            x: super::XPAD as f64,
            y: (style_options.imageheight - (super::YPAD2 / 2)) as f64,
            text: " ",
            location: None,
            extra: iter::once(("id", "details")),
        },
    )?;

    write_str(
        svg,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: super::FONTSIZE,
            x: super::XPAD as f64,
            y: (super::FONTSIZE * 2) as f64,
            text: "Reset Zoom",
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
        TextItem {
            color: "rgb(0, 0, 0)",
            size: super::FONTSIZE,
            x: (super::IMAGEWIDTH - super::XPAD - 100) as f64,
            y: (super::FONTSIZE * 2) as f64,
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

    write_str(
        svg,
        TextItem {
            color: "rgb(0, 0, 0)",
            size: super::FONTSIZE,
            x: (super::IMAGEWIDTH - super::XPAD - 100) as f64,
            y: (style_options.imageheight - (super::YPAD2 / 2)) as f64,
            text: " ",
            location: None,
            extra: iter::once(("id", "matched")),
        },
    )?;

    Ok(())
}

pub(super) fn write_str<'a, W, I>(
    svg: &mut Writer<W>,
    item: TextItem<'a, I>,
) -> quick_xml::Result<()>
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
