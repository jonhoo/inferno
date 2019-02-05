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
        if name.trim().is_empty() {
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
