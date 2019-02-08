use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;

macro_rules! unwrap_or_continue {
    ($e:expr) => {{
        if let Some(x) = $e {
            x
        } else {
            continue;
        }
    }};
}

/// Provides a way to customize the attributes on the SVG elements for a frame.
#[derive(PartialEq, Eq, Debug, Default)]
pub struct FuncFrameAttrsMap(HashMap<String, FrameAttrs>);

impl FuncFrameAttrsMap {
    /// Parse a FuncFrameAttrsMap from a file.
    /// Each line should be a function name followed by a tab,
    /// then a sequence of tab separated name=value pairs.
    pub fn from_file(path: &PathBuf) -> io::Result<FuncFrameAttrsMap> {
        let file = BufReader::new(File::open(path)?);
        FuncFrameAttrsMap::from_reader(file)
    }

    /// Parse a FuncFrameAttrsMap from a reader.
    /// Each line should be a function name followed by a tab,
    /// then a sequence of tab separated name=value pairs.
    pub fn from_reader<R: BufRead>(mut reader: R) -> io::Result<FuncFrameAttrsMap> {
        let mut funcattr_map = FuncFrameAttrsMap::default();
        let mut line = String::new();
        loop {
            line.clear();

            if reader.read_line(&mut line)? == 0 {
                break;
            }

            let mut line = line.trim().splitn(2, '\t');
            let func = unwrap_or_continue!(line.next());
            if func.is_empty() {
                continue;
            }
            let funcattrs = funcattr_map.0.entry(func.to_string()).or_default();
            let namevals = unwrap_or_continue!(line.next());
            for nameval in namevals.split('\t') {
                let mut nameval = nameval.splitn(2, '=');
                let name = unwrap_or_continue!(nameval.next());
                if name.is_empty() {
                    continue;
                }
                let value = unwrap_or_continue!(nameval.next()).to_string();
                match name {
                    "title" => funcattrs.title = Some(value),
                    "class" => funcattrs.g.class = Some(value),
                    "style" => funcattrs.g.style = Some(value),
                    "onmouseover" => funcattrs.g.onmouseover = Some(value),
                    "onmouseout" => funcattrs.g.onmouseout = Some(value),
                    "onclick" => funcattrs.g.onclick = Some(value),
                    "href" => funcattrs.a.href = Some(value),
                    "target" => funcattrs.a.target = Some(value),
                    "g_extra" => parse_extra_attrs(&mut funcattrs.g.extra, &value),
                    "a_extra" => parse_extra_attrs(&mut funcattrs.a.extra, &value),
                    _ => warn!("invalid attribute {} found for {}", name, func),
                }
            }
        }

        Ok(funcattr_map)
    }

    /// Return FrameAttrs for the given function name if it exists
    pub fn frameattrs_for_func(&self, func: &str) -> Option<&FrameAttrs> {
        self.0.get(func)
    }
}

/// Attributes to set on the SVG elements of a frame
#[derive(PartialEq, Eq, Debug, Default)]
pub struct FrameAttrs {
    /// The text to include in the `title` element.
    /// If set to None, the title is dynamically generated based on the function name.
    pub title: Option<String>,

    pub g: GElementAttrs,
    pub a: AElementAttrs,
}

/// Attributes to set on the SVG `g` element.
/// Any of them set to `None` will get the default value.
#[derive(PartialEq, Eq, Debug, Default)]
pub struct GElementAttrs {
    /// Defaults to "func_g"
    pub class: Option<String>,

    /// Will not be included if None
    pub style: Option<String>,

    /// Defaults to "s(this)"
    pub onmouseover: Option<String>,

    /// Defaults to "c()"
    pub onmouseout: Option<String>,

    /// Defaults to "zoom(this)"
    pub onclick: Option<String>,

    /// Extra attributes to include
    pub extra: HashMap<String, String>,
}

/// Attributes to set on the SVG `a` element
#[derive(PartialEq, Eq, Debug, Default)]
pub struct AElementAttrs {
    /// If set to None the `a` tag will not be added
    pub href: Option<String>,

    /// Defaults to "_top"
    pub target: Option<String>,

    /// Extra attributes to include
    pub extra: HashMap<String, String>,
}

fn parse_extra_attrs(attrs: &mut HashMap<String, String>, s: &str) {
    attrs.extend(AttrIter { s });
}

struct AttrIter<'a> {
    s: &'a str,
}

impl<'a> Iterator for AttrIter<'a> {
    type Item = (String, String);

    fn next(&mut self) -> Option<(String, String)> {
        let equal = self.s.find('=')?;
        let open_quote = self.s[equal..].find('"')?;
        let end_quote = self.s[equal + open_quote + 1..].find('"')?;
        let mut nameval = self.s[..equal + open_quote + end_quote + 2].splitn(2, '=');
        let name = nameval.next()?.trim();
        if !name.is_empty() {
            self.s = &self.s[equal + open_quote + end_quote + 2..];
            let value = nameval.next()?.trim().trim_matches('"');
            return Some((name.to_string(), value.to_string()));
        }

        None
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn funcattr_map_from_reader() {
        let foo = vec![
            "foo",
            "title=foo title",
            "class=foo class",
            "style=foo style",
            "onmouseover=foo_onmouseover()",
            "onmouseout=foo_onmouseout()",
            "onclick=foo_onclick()",
            r#"g_extra=gextra1="foo gextra1" gextra2="foo gextra2""#,
            "href=foo href",
            "target=foo target",
            r#"a_extra=aextra1="foo aextra1" aextra2="foo aextra2""#,
        ]
        .join("\t");

        let bar = vec![
            "bar",
            "class=bar class",
            "onmouseover=bar_onmouseover()",
            "href=bar href",
            r#"a_extra=aextra="bar aextra""#,
        ]
        .join("\t");

        let s = vec![foo, bar].join("\n");
        let r = s.as_bytes();

        let mut expected_inner = HashMap::new();
        let mut foo_g_extra = HashMap::new();
        foo_g_extra.insert("gextra1".to_owned(), "foo gextra1".to_owned());
        foo_g_extra.insert("gextra2".to_owned(), "foo gextra2".to_owned());
        let mut foo_a_extra = HashMap::new();
        foo_a_extra.insert("aextra1".to_owned(), "foo aextra1".to_owned());
        foo_a_extra.insert("aextra2".to_owned(), "foo aextra2".to_owned());

        expected_inner.insert(
            "foo".to_owned(),
            FrameAttrs {
                title: Some("foo title".to_owned()),
                g: GElementAttrs {
                    class: Some("foo class".to_owned()),
                    style: Some("foo style".to_owned()),
                    onmouseover: Some("foo_onmouseover()".to_owned()),
                    onmouseout: Some("foo_onmouseout()".to_owned()),
                    onclick: Some("foo_onclick()".to_owned()),
                    extra: foo_g_extra,
                },
                a: AElementAttrs {
                    href: Some("foo href".to_owned()),
                    target: Some("foo target".to_owned()),
                    extra: foo_a_extra,
                },
            },
        );

        let mut bar_a_extra = HashMap::new();
        bar_a_extra.insert("aextra".to_owned(), "bar aextra".to_owned());

        expected_inner.insert(
            "bar".to_owned(),
            FrameAttrs {
                title: None,
                g: GElementAttrs {
                    class: Some("bar class".to_owned()),
                    style: None,
                    onmouseover: Some("bar_onmouseover()".to_owned()),
                    onmouseout: None,
                    onclick: None,
                    extra: HashMap::default(),
                },
                a: AElementAttrs {
                    href: Some("bar href".to_owned()),
                    target: None,
                    extra: bar_a_extra,
                },
            },
        );

        let result = FuncFrameAttrsMap::from_reader(r).unwrap();
        let expected = FuncFrameAttrsMap(expected_inner);

        assert_eq!(result, expected);
    }
}
