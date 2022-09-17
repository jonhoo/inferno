use std::borrow::Cow;
use std::cell::RefCell;
use std::io::prelude::*;
use std::iter;

use quick_xml::events::{BytesCData, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use str_stack::StrStack;

use super::{Direction, Options, TextTruncateDirection};

/// The generic font families should not have quotes around them in the CSS.
const GENERIC_FONT_FAMILIES: &[&str] = &["cursive", "fantasy", "monospace", "serif", "sans-serif"];

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

pub(super) enum Dimension {
    Pixels(usize),
    Percent(f64),
}

pub(super) struct TextItem<'a, I> {
    pub(super) x: Dimension,
    pub(super) y: f64,
    pub(super) text: TextArgument<'a>,
    pub(super) extra: I,
}

pub(super) struct StyleOptions<'a> {
    pub(super) imageheight: usize,
    pub(super) bgcolor1: Cow<'a, str>,
    pub(super) bgcolor2: Cow<'a, str>,
    pub(super) strokecolor: Option<String>,
}

pub(super) fn write_header<W>(
    svg: &mut Writer<W>,
    imageheight: usize,
    opt: &Options<'_>,
) -> quick_xml::Result<()>
where
    W: Write,
{
    svg.write(br#"<?xml version="1.0" standalone="no"?>"#)?;
    svg.write(br#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">"#)?;
    let imagewidth = opt.image_width.unwrap_or(super::DEFAULT_IMAGE_WIDTH);
    svg.write_event(Event::Start(
        BytesStart::borrowed_name(b"svg").with_attributes(vec![
            ("version", "1.1"),
            ("width", &*format!("{}", imagewidth)),
            ("height", &*format!("{}", imageheight)),
            ("onload", "init(evt)"),
            ("viewBox", &*format!("0 0 {} {}", imagewidth, imageheight)),
            ("xmlns", "http://www.w3.org/2000/svg"),
            ("xmlns:xlink", "http://www.w3.org/1999/xlink"),
            ("xmlns:fg", "http://github.com/jonhoo/inferno"),
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
    opt: &Options<'_>,
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

    let font_type: Cow<str> = if GENERIC_FONT_FAMILIES.contains(&opt.font_type.as_str()) {
        Cow::Borrowed(&opt.font_type)
    } else {
        Cow::Owned(enquote('\"', &opt.font_type))
    };

    let titlesize = &opt.font_size + 5;
    svg.write_event(Event::Text(BytesText::from_escaped_str(&format!(
        "
text {{ font-family:{}; font-size:{}px; fill:rgb(0,0,0); }}
#title {{ text-anchor:middle; font-size:{}px; }}
",
        font_type, &opt.font_size, titlesize,
    ))))?;
    if let Some(strokecolor) = &style_options.strokecolor {
        svg.write_event(Event::Text(BytesText::from_escaped_str(&format!(
            "#frames > g > rect {{ stroke:{}; stroke-width:1; }}\n",
            strokecolor
        ))))?;
    }
    svg.write_event(Event::Text(BytesText::from_escaped_str(include_str!(
        "flamegraph.css"
    ))))?;
    svg.write_event(Event::End(BytesEnd::borrowed(b"style")))?;

    svg.write_event(Event::Start(
        BytesStart::borrowed_name(b"script")
            .with_attributes(iter::once(("type", "text/ecmascript"))),
    ))?;
    svg.write_event(Event::CData(BytesCData::from_str(&format!(
        "
        var nametype = {};
        var fontsize = {};
        var fontwidth = {};
        var xpad = {};
        var inverted = {};
        var searchcolor = '{}';
        var fluiddrawing = {};
        var truncate_text_right = {};\n    ",
        enquote('\'', &opt.name_type),
        opt.font_size,
        opt.font_width,
        super::XPAD,
        opt.direction == Direction::Inverted,
        opt.search_color,
        opt.image_width.is_none(),
        opt.text_truncate_direction == TextTruncateDirection::Right
    ))))?;
    if !opt.no_javascript {
        svg.write_event(Event::CData(BytesCData::from_str(include_str!(
            "flamegraph.js"
        ))))?;
    }
    svg.write_event(Event::End(BytesEnd::borrowed(b"script")))?;

    svg.write_event(Event::Empty(
        BytesStart::borrowed_name(b"rect").with_attributes(vec![
            ("x", "0"),
            ("y", "0"),
            ("width", "100%"),
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
            x: Dimension::Percent(50.0),
            y: (opt.font_size * 2) as f64,
            text: (&*opt.title).into(),
            extra: vec![("id", "title")],
        },
    )?;

    if let Some(ref subtitle) = opt.subtitle {
        write_str(
            svg,
            &mut buf,
            TextItem {
                x: Dimension::Percent(50.0),
                y: (opt.font_size * 4) as f64,
                text: (&**subtitle).into(),
                extra: vec![("id", "subtitle")],
            },
        )?;
    }

    let image_width = opt.image_width.unwrap_or(super::DEFAULT_IMAGE_WIDTH) as f64;

    write_str(
        svg,
        &mut buf,
        TextItem {
            x: Dimension::Pixels(super::XPAD),
            y: if opt.direction == Direction::Straight {
                style_options.imageheight - (opt.ypad2() / 2)
            } else {
                // Inverted (icicle) mode, put the details on top:
                opt.ypad1() - opt.font_size
            } as f64,
            text: " ".into(),
            extra: iter::once(("id", "details")),
        },
    )?;

    write_str(
        svg,
        &mut buf,
        TextItem {
            x: Dimension::Pixels(super::XPAD),
            y: (opt.font_size * 2) as f64,
            text: "Reset Zoom".into(),
            extra: vec![("id", "unzoom"), ("class", "hide")],
        },
    )?;

    write_str(
        svg,
        &mut buf,
        TextItem {
            x: Dimension::Pixels(image_width as usize - super::XPAD),
            y: (opt.font_size * 2) as f64,
            text: "Search".into(),
            extra: vec![("id", "search")],
        },
    )?;

    write_str(
        svg,
        &mut buf,
        TextItem {
            x: Dimension::Pixels(image_width as usize - super::XPAD),
            y: (style_options.imageheight - (opt.ypad2() / 2)) as f64,
            text: " ".into(),
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
    let x = match item.x {
        Dimension::Pixels(x) => write!(buf, "{:.2}", x),
        Dimension::Percent(x) => write!(buf, "{:.4}%", x),
    };
    let y = write!(buf, "{:.2}", item.y);

    let TextItem { text, extra, .. } = item;

    thread_local! {
        // reuse for all text elements to avoid allocations
        static TEXT: RefCell<Event<'static>> = RefCell::new(Event::Start(BytesStart::owned_name("text")))
    };
    TEXT.with(|start_event| {
        if let Event::Start(ref mut text) = *start_event.borrow_mut() {
            text.clear_attributes();
            text.extend_attributes(extra);
            text.extend_attributes(args!(
                "x" => &buf[x],
                "y" => &buf[y]
            ));
        } else {
            unreachable!("cache wrapper was of wrong type: {:?}", start_event);
        }

        svg.write_event(&*start_event.borrow())
    })?;
    let s = match text {
        TextArgument::String(ref s) => s,
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
