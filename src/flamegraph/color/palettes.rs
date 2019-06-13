pub(super) mod java {
    use crate::flamegraph::color::BasicPalette;

    /// Handle both annotations (_[j], _[i], ...; which are
    /// accurate), as well as input that lacks any annotations, as
    /// best as possible. Without annotations, we get a little hacky
    /// and match on java|org|com, etc.
    pub fn resolve(name: &str) -> BasicPalette {
        if name.ends_with(']') {
            if let Some(ai) = name.rfind("_[") {
                if name[ai..].len() == 4 {
                    match &name[ai + 2..ai + 3] {
                        // kernel annotation
                        "k" => return BasicPalette::Orange,
                        // inline annotation
                        "i" => return BasicPalette::Aqua,
                        // jit annotation
                        "j" => return BasicPalette::Green,
                        _ => {}
                    }
                }
            }
        }

        let java_prefix = if name.starts_with('L') {
            &name[1..]
        } else {
            name
        };

        if java_prefix.starts_with("java/")
            || java_prefix.starts_with("javax/")
            || java_prefix.starts_with("jdk/")
            || java_prefix.starts_with("net/")
            || java_prefix.starts_with("org/")
            || java_prefix.starts_with("com/")
            || java_prefix.starts_with("io/")
            || java_prefix.starts_with("sun/")
        {
            // Java
            BasicPalette::Green
        } else if name.contains("::") {
            // C++
            BasicPalette::Yellow
        } else {
            // system
            BasicPalette::Red
        }
    }
}

pub(super) mod perl {
    use crate::flamegraph::color::BasicPalette;

    pub fn resolve(name: &str) -> BasicPalette {
        if name.ends_with("_[k]") {
            BasicPalette::Orange
        } else if name.contains("Perl") || name.contains(".pl") {
            BasicPalette::Green
        } else if name.contains("::") {
            BasicPalette::Yellow
        } else {
            BasicPalette::Red
        }
    }
}

pub(super) mod js {
    use crate::flamegraph::color::BasicPalette;

    pub fn resolve(name: &str) -> BasicPalette {
        if !name.is_empty() && name.trim().is_empty() {
            return BasicPalette::Green;
        } else if name.ends_with("_[k]") {
            return BasicPalette::Orange;
        } else if name.ends_with("_[j]") {
            if name.contains('/') {
                return BasicPalette::Green;
            } else {
                return BasicPalette::Aqua;
            }
        } else if name.contains("::") {
            return BasicPalette::Yellow;
        } else if name.contains(':') {
            return BasicPalette::Aqua;
        } else if let Some(ai) = name.find('/') {
            if (&name[ai..]).contains(".js") {
                return BasicPalette::Green;
            }
        }

        BasicPalette::Red
    }
}

pub(super) mod wakeup {
    use crate::flamegraph::color::BasicPalette;

    pub fn resolve(_name: &str) -> BasicPalette {
        BasicPalette::Aqua
    }
}

#[cfg(test)]
mod tests {
    use crate::flamegraph::color::BasicPalette;

    struct TestData {
        input: String,
        output: BasicPalette,
    }

    #[test]
    fn perl_mod_resolves() {
        use super::perl::resolve;

        let test_names = [
            TestData {
                input: String::from(" "),
                output: BasicPalette::Red,
            },
            TestData {
                input: String::from(""),
                output: BasicPalette::Red,
            },
            TestData {
                input: String::from("something"),
                output: BasicPalette::Red,
            },
            TestData {
                input: String::from("somethingpl"),
                output: BasicPalette::Red,
            },
            TestData {
                input: String::from("something/_[k]"),
                output: BasicPalette::Orange,
            },
            TestData {
                input: String::from("something_[k]"),
                output: BasicPalette::Orange,
            },
            TestData {
                input: String::from("some::thing"),
                output: BasicPalette::Yellow,
            },
            TestData {
                input: String::from("some/ai.pl"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("someai.pl"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("something/Perl"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("somethingPerl"),
                output: BasicPalette::Green,
            },
        ];

        for item in test_names.iter() {
            let resolved_color = resolve(&item.input);
            assert_eq!(resolved_color, item.output)
        }
    }
}
