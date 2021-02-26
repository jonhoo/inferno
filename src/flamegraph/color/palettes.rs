enum Annotation {
    Kernel,
    Inline,
    Jit,
}

fn resolve_annotation(name: &str) -> Option<Annotation> {
    if name.ends_with(']') {
        if let Some(ai) = name.rfind("_[") {
            if name[ai..].len() == 4 {
                match &name[ai + 2..ai + 3] {
                    "k" => return Some(Annotation::Kernel),
                    "i" => return Some(Annotation::Inline),
                    "j" => return Some(Annotation::Jit),
                    _ => {}
                }
            }
        }
    }

    None
}

pub(super) mod annotated {
    use super::Annotation;
    use crate::flamegraph::color::BasicPalette;

    /// Handle annotations (_[j], _[i], ...; which are
    /// accurate) in a generic way.
    pub fn resolve(name: &str) -> BasicPalette {
        match super::resolve_annotation(name) {
            Some(Annotation::Kernel) => BasicPalette::Gray,
            Some(Annotation::Inline) => BasicPalette::Aqua,
            Some(Annotation::Jit) => BasicPalette::Green,
            None => BasicPalette::Hot,
        }
    }
}

pub(super) mod java {
    use super::Annotation;
    use crate::flamegraph::color::BasicPalette;

    /// Handle both annotations (_[j], _[i], ...; which are
    /// accurate), as well as input that lacks any annotations, as
    /// best as possible. Without annotations, we get a little hacky
    /// and match on java|org|com, etc.
    pub fn resolve(name: &str) -> BasicPalette {
        if let Some(annotation) = super::resolve_annotation(name) {
            return match annotation {
                Annotation::Kernel => BasicPalette::Orange,
                Annotation::Inline => BasicPalette::Aqua,
                Annotation::Jit => BasicPalette::Green,
            };
        }

        let java_prefix = if name.starts_with('L') {
            &name[1..]
        } else {
            name
        };

        if name.contains("::") || name.starts_with("-[") || name.starts_with("+[") {
            // C++ or Objective C
            BasicPalette::Yellow
        } else if java_prefix.contains('/')
            || (java_prefix.contains('.') && !java_prefix.starts_with('['))
            || match java_prefix.chars().next() {
                Some(c) => c.is_ascii_uppercase(),
                _ => false,
            }
        {
            // Java
            BasicPalette::Green
        } else {
            // system
            BasicPalette::Red
        }
    }
}

pub(super) mod perl {
    use crate::flamegraph::color::BasicPalette;

    pub fn resolve(name: &str) -> BasicPalette {
        if let Some(super::Annotation::Kernel) = super::resolve_annotation(name) {
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
        let annotation = super::resolve_annotation(name);

        if !name.is_empty() && name.trim().is_empty() {
            return BasicPalette::Green;
        } else if let Some(super::Annotation::Kernel) = annotation {
            return BasicPalette::Orange;
        } else if let Some(super::Annotation::Jit) = annotation {
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
    fn java_mod_resolves() {
        use super::java::resolve;

        let test_names = [
            TestData {
                input: String::from("_[k]"),
                output: BasicPalette::Orange,
            },
            TestData {
                input: String::from("_[j]_[k]"),
                output: BasicPalette::Orange,
            },
            TestData {
                input: String::from("_[]_[k]"),
                output: BasicPalette::Orange,
            },
            TestData {
                input: String::from("_[j]"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("_[k]_[j]"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("_[]_[j]"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("_[i]"),
                output: BasicPalette::Aqua,
            },
            TestData {
                input: String::from("_[j]_[i]"),
                output: BasicPalette::Aqua,
            },
            TestData {
                input: String::from("_[]_[i]"),
                output: BasicPalette::Aqua,
            },
            TestData {
                input: String::from("_[j]_[]"),
                output: BasicPalette::Red,
            },
            TestData {
                input: String::from("_[j]_[jj]"),
                output: BasicPalette::Red,
            },
            TestData {
                input: String::from("_[jk]"),
                output: BasicPalette::Red,
            },
            TestData {
                input: String::from("_[i]blah"),
                output: BasicPalette::Red,
            },
            TestData {
                input: String::from("java/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("java/somestuff"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("javax/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("javax/somestuff"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("jdk/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("jdk/somestuff"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("net/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("net/somestuff"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("org/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("org/somestuff"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("com/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("com/somestuff"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("io/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("io/somestuff"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("sun/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("sun/somestuff"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("Ljava/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("Ljavax/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("Ljdk/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("Lnet/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("Lorg/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("Lcom/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("Lio/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("Lsun/"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("jdk/_[ki]"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("jdk/::[ki]"),
                output: BasicPalette::Yellow,
            },
            TestData {
                input: String::from("Ajdk/_[ki]"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("Ajdk/::[ki]"),
                output: BasicPalette::Yellow,
            },
            TestData {
                input: String::from("jdk::[ki]"),
                output: BasicPalette::Yellow,
            },
            TestData {
                input: String::from("::[ki]"),
                output: BasicPalette::Yellow,
            },
            TestData {
                input: String::from("::"),
                output: BasicPalette::Yellow,
            },
            TestData {
                input: String::from("some::st_[jk]uff"),
                output: BasicPalette::Yellow,
            },
            TestData {
                input: String::from("jdk"),
                output: BasicPalette::Red,
            },
            TestData {
                input: String::from("Ljdk"),
                output: BasicPalette::Red,
            },
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
                input: String::from("some:thing"),
                output: BasicPalette::Red,
            },
            TestData {
                input: String::from("scala.tools.nsc.Global$Run.compile"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("sbt.execute.work"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("org.scalatest.Suit.run"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("Compile"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("-[test]"),
                output: BasicPalette::Yellow,
            },
            TestData {
                input: String::from("+[test]"),
                output: BasicPalette::Yellow,
            },
            TestData {
                input: String::from("[test.event]"),
                output: BasicPalette::Red,
            },
        ];

        for item in test_names.iter() {
            let resolved_color = resolve(&item.input);
            assert_eq!(resolved_color, item.output)
        }
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

    #[test]
    fn js_returns_correct() {
        use super::js;

        let test_data = [
            TestData {
                input: String::from(" "),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("something_[k]"),
                output: BasicPalette::Orange,
            },
            TestData {
                input: String::from("something/_[j]"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("something_[j]"),
                output: BasicPalette::Aqua,
            },
            TestData {
                input: String::from("some::thing"),
                output: BasicPalette::Yellow,
            },
            TestData {
                input: String::from("some:thing"),
                output: BasicPalette::Aqua,
            },
            TestData {
                input: String::from("some/ai.js"),
                output: BasicPalette::Green,
            },
            TestData {
                input: String::from("someai.js"),
                output: BasicPalette::Red,
            },
        ];
        for elem in test_data.iter() {
            let result = js::resolve(&elem.input);
            assert_eq!(result, elem.output);
        }
    }
}
