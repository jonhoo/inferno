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
    /// Parse frame attributes from a file.
    ///
    /// Each line should consist of a function name, a tab (`\t`), and then a sequence of
    /// tab-separated `name=value` pairs.
    pub fn from_file(path: &PathBuf) -> io::Result<FuncFrameAttrsMap> {
        let file = BufReader::new(File::open(path)?);
        FuncFrameAttrsMap::from_reader(file)
    }

    /// Parse frame attributes from a `BufRead`.
    ///
    /// Each line should consist of a function name, a tab (`\t`), and then a sequence of
    /// tab-separated `name=value` pairs.
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
                let name = unwrap_or_continue!(nameval.next()).trim();
                if name.is_empty() {
                    continue;
                }
                let mut value = unwrap_or_continue!(nameval.next()).trim();
                // Remove optional quotes
                if value.starts_with('"') && value.ends_with('"') {
                    value = &value[1..value.len() - 1];
                }
                match name {
                    "title" => funcattrs.title = Some(value.to_string()),
                    "id" => funcattrs.g.id = Some(value.to_string()),
                    "class" => funcattrs.g.class = Some(value.to_string()),
                    "href" => funcattrs.a.href = Some(value.to_string()),
                    "target" => funcattrs.a.target = Some(value.to_string()),
                    "g_extra" => parse_extra_attrs(&mut funcattrs.g.extra, value),
                    "a_extra" => parse_extra_attrs(&mut funcattrs.a.extra, value),
                    _ => warn!("invalid attribute {} found for {}", name, func),
                }
            }
        }

        Ok(funcattr_map)
    }

    /// Return FrameAttrs for the given function name if it exists
    pub(super) fn frameattrs_for_func(&self, func: &str) -> Option<&FrameAttrs> {
        self.0.get(func)
    }
}

/// Attributes to set on the SVG elements of a frame
#[derive(PartialEq, Eq, Debug, Default)]
pub(super) struct FrameAttrs {
    /// The text to include in the `title` element.
    /// If set to None, the title is dynamically generated based on the function name.
    pub(super) title: Option<String>,

    pub(super) g: GElementAttrs,
    pub(super) a: AElementAttrs,
}

/// Attributes to set on the SVG `g` element.
/// Any of them set to `None` will get the default value.
#[derive(PartialEq, Eq, Debug, Default)]
pub(super) struct GElementAttrs {
    /// Will not be included if None
    pub(super) class: Option<String>,

    /// Will not be included if None
    pub(super) id: Option<String>,

    /// Extra attributes to include
    pub(super) extra: Vec<(String, String)>,
}

/// Attributes to set on the SVG `a` element
#[derive(PartialEq, Eq, Debug, Default)]
pub(super) struct AElementAttrs {
    /// If set to None the `a` tag will not be added
    pub(super) href: Option<String>,

    /// Defaults to "_top"
    pub(super) target: Option<String>,

    /// Extra attributes to include
    pub(super) extra: Vec<(String, String)>,
}

fn parse_extra_attrs(attrs: &mut Vec<(String, String)>, s: &str) {
    attrs.extend(AttrIter { s });
}

struct AttrIter<'a> {
    s: &'a str,
}

impl<'a> Iterator for AttrIter<'a> {
    type Item = (String, String);

    fn next(&mut self) -> Option<(String, String)> {
        let mut name_rest = self.s.splitn(2, '=');
        let name = name_rest.next()?.trim();
        if name.is_empty() {
            warn!("\"=\" found with no name in extra attributes");
            return None;
        }
        let mut split_name = name.split_whitespace();
        let name = split_name.next_back()?;
        for extra in split_name {
            warn!(
                "extra attribute {} has no value (did you mean to quote the value?)",
                extra
            );
        }

        let rest = name_rest.next()?.trim_start();
        if rest.is_empty() {
            warn!("no value after \"=\" for extra attribute {}", name);
        }

        let (value, rest) = if rest.starts_with('"') {
            if let Some(eq) = rest[1..].find('"') {
                (&rest[1..=eq], &rest[eq + 1..])
            } else {
                warn!("no end quote found for extra attribute {}", name);
                return None;
            }
        } else if let Some(w) = rest.find(char::is_whitespace) {
            (&rest[..w], &rest[w + 1..])
        } else {
            (rest, "")
        };

        self.s = rest;

        Some((name.to_string(), value.to_string()))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn func_frame_attrs_map_from_reader() {
        let foo = vec![
            "foo",
            // Without quotes
            "title=foo title",
            // With quotes
            r#"class="foo class""#,
            // gextra1 without quotes, gextra2 with quotes
            r#"g_extra=gextra1=gextra1 gextra2="foo gextra2""#,
            "href=foo href",
            "target=foo target",
            // Extra quotes around a_extra value
            r#"a_extra="aextra1="foo aextra1" aextra2="foo aextra2"""#,
        ]
        .join("\t");

        let bar = vec![
            "bar",
            "class=bar class",
            "href=bar href",
            // With an invalid attribute that has no value
            // This gets skipped and logged.
            r#"a_extra=aextra1=foo invalid aextra2=bar"#,
        ]
        .join("\t");

        let s = vec![foo, bar].join("\n");
        let r = s.as_bytes();

        let mut expected_inner = HashMap::new();
        let foo_g_extra: Vec<(String, String)> = vec![
            ("gextra1".to_owned(), "gextra1".to_owned()),
            ("gextra2".to_owned(), "foo gextra2".to_owned()),
        ];
        let foo_a_extra: Vec<(String, String)> = vec![
            ("aextra1".to_owned(), "foo aextra1".to_owned()),
            ("aextra2".to_owned(), "foo aextra2".to_owned()),
        ];

        expected_inner.insert(
            "foo".to_owned(),
            FrameAttrs {
                title: Some("foo title".to_owned()),
                g: GElementAttrs {
                    id: None,
                    class: Some("foo class".to_owned()),
                    extra: foo_g_extra,
                },
                a: AElementAttrs {
                    href: Some("foo href".to_owned()),
                    target: Some("foo target".to_owned()),
                    extra: foo_a_extra,
                },
            },
        );

        let bar_a_extra: Vec<(String, String)> = vec![
            ("aextra1".to_owned(), "foo".to_owned()),
            ("aextra2".to_owned(), "bar".to_owned()),
        ];

        expected_inner.insert(
            "bar".to_owned(),
            FrameAttrs {
                title: None,
                g: GElementAttrs {
                    id: None,
                    class: Some("bar class".to_owned()),
                    extra: Vec::default(),
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
