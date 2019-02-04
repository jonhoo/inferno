use std::str::FromStr;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::fs::File;
use std::io::BufReader;
use std::io::BufRead;

pub(super) const VDGREY: &str = "rgb(160,160,160)";
pub(super) const DGREY: &str = "rgb(200,200,200)";

const YELLOW_GRADIENT: (&str, &str) = ("#eeeeee", "#eeeeb0");
const BLUE_GRADIENT: (&str, &str) = ("#eeeeee", "#e0e0ff");
const GRAY_GRADIENT: (&str, &str) = ("#f8f8f8", "#e8e8e8");

#[derive(Debug, PartialEq)]
pub enum Palette {
    Hot,
    Mem,
    Io,
    Wakeup,
    Chain,
    Java,
    Js,
    Perl,
    Red,
    Green,
    Blue,
    Aqua,
    Yellow,
    Purple,
    Orange,
}

impl Default for Palette {
    fn default() -> Self {
        Palette::Hot
    }
}

impl FromStr for Palette {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "hot" => Ok(Palette::Hot),
            "mem" => Ok(Palette::Mem),
            "io" => Ok(Palette::Io),
            "wakeup" => Ok(Palette::Wakeup),
            "chain" => Ok(Palette::Chain),
            "java" => Ok(Palette::Java),
            "js" => Ok(Palette::Js),
            "perl" => Ok(Palette::Perl),
            "red" => Ok(Palette::Red),
            "green" => Ok(Palette::Green),
            "blue" => Ok(Palette::Blue),
            "aqua" => Ok(Palette::Aqua),
            "yellow" => Ok(Palette::Yellow),
            "purple" => Ok(Palette::Purple),
            "orange" => Ok(Palette::Orange),
            unknown=> Err(format!("unknown color palette: {}", unknown))
        }
    }
}

/// Generate a vector hash for the name string, weighting early over
/// later characters. We want to pick the same colors for function
/// names across different flame graphs.
fn namehash(name: &str) -> f32 {
    let mut vector = 0.0;
    let mut weight = 1.0;
    let mut max = 1.0;
    let mut modulo = 10;

    let name = {
        let looking_for_module_name = if name.starts_with("`") {
            &name[1..]
        } else {
            name
        };

        if let Some(index) = looking_for_module_name.find("`") {
            &looking_for_module_name[index + 1..]
        } else {
            name
        }
    };

    for character in name.bytes().take(3) {
        let i = (character % modulo) as f32;
        vector += (i / ((modulo - 1) as f32)) * weight;
        modulo += 1;
        max += weight;
        weight *= 0.70;
    }

    (1.0 - vector / max)
}

/// Handle both annotations (_[j], _[i], ...; which are
/// accurate), as well as input that lacks any annotations, as
/// best as possible. Without annotations, we get a little hacky
/// and match on java|org|com, etc.
fn handle_java_palette(s: &str) -> Palette {
    if s.ends_with("]") {
        if let Some(ai) = s.rfind("_[") {
            if s[ai..].len() == 4 {
                match &s[ai+2..ai+3] {
                    // kernel annotation
                    "k" => return Palette::Orange,
                    // inline annotation
                    "i" => return Palette::Aqua,
                    // jit annotation
                    "j" => return Palette::Green,
                    _ => {},
                }
            }
        }
    }

    let java_prefix = if s.starts_with("L") { &s[1..] } else { s };

    if java_prefix.starts_with("java/") ||
        java_prefix.starts_with("org/") ||
        java_prefix.starts_with("com/") ||
        java_prefix.starts_with("io/") ||
        java_prefix.starts_with("sun/") {
        // Java
        Palette::Green
    } else if s.contains("::") {
        // C++
        Palette::Yellow
    } else {
        // system
        Palette::Red
    }
}

fn handle_perl_palette(s: &str) -> Palette {
    if s.ends_with("_[k]") {
        Palette::Orange
    } else if s.contains("Perl") || s.contains(".pl") {
        Palette::Green
    } else if s.contains("::") {
        Palette::Yellow
    } else {
        Palette::Red
    }
}

fn handle_js_palette(s: &str) -> Palette {
    if s.trim().is_empty() {
        return Palette::Green
    } else if s.ends_with("_[k]") {
        return Palette::Orange
    } else if s.contains("::") {
        return Palette::Yellow
    } else if s.contains(":") {
        return Palette::Aqua
    } else if let Some(ai) = s.find("/") {
        if (&s[ai..]).contains(".js") {
            return Palette::Green
        }
    } else if s.ends_with("_[j]") {
        if s.contains("/") {
            return Palette::Green
        } else {
            return Palette::Aqua
        }
    }

    Palette::Red
}

fn handle_wakeup_palette(_s: &str) -> Palette {
    Palette::Aqua
}

fn handle_chain_palette(s: &str) -> Palette {
    if s.contains("_[w]") {
        Palette::Aqua
    } else {
        Palette::Blue
    }
}

macro_rules! t {
    ($b:expr, $a:expr, $x:expr) => ($b + ($a as f32 * $x) as u8)
}

fn rgb_components_for_palette(palette: &Palette, name: &str, v1: f32, v2: f32, v3: f32) -> (u8, u8, u8) {
    let real_palette = match palette {
        Palette::Hot => return (t!(205, 50, v3), t!(0, 230, v1), t!(0, 55, v2)),
        Palette::Mem => return (t!(0, 0, v3), t!(190, 50, v2), t!(0, 210, v1)),
        Palette::Io => return (t!(80, 60, v1), t!(80, 60, v1), t!(190, 55, v2)),
        Palette::Red => return (t!(200, 55, v1), t!(50, 80, v1), t!(50, 80, v1)),
        Palette::Green => return (t!(50, 60, v1), t!(200, 55, v1), t!(50, 60, v1)),
        Palette::Blue => return (t!(80, 60, v1), t!(80, 60, v1), t!(205, 50, v1)),
        Palette::Yellow => return (t!(175, 55, v1), t!(175, 55, v1), t!(50, 20, v1)),
        Palette::Purple => return (t!(190, 65, v1), t!(80, 60, v1), t!(190, 65, v1)),
        Palette::Aqua => return (t!(50, 60, v1), t!(165, 55, v1), t!(165, 55, v1)),
        Palette::Orange => return (t!(190, 65, v1), t!(90, 65, v1), t!(0, 0, v1)),
        Palette::Java => handle_java_palette(name),
        Palette::Perl => handle_perl_palette(name),
        Palette::Js => handle_js_palette(name),
        Palette::Wakeup => handle_wakeup_palette(name),
        Palette::Chain => handle_chain_palette(name),
    };

    rgb_components_for_palette(&real_palette, name, v1, v2, v3)
}

fn color_from_palette(palette: &Palette, name: &str, v1: f32, v2: f32, v3: f32) -> String {
    let (red, green, blue) = rgb_components_for_palette(palette, name, v1, v2, v3);

    format!("rgb({},{},{})", red, green, blue)
}

pub(super) fn color_map<'a>(palette: &Palette, hash: bool, name: &'a str, palette_map: &'a mut HashMap<String, String>) -> &'a str {
    palette_map.entry(name.to_string()).or_insert_with(|| color(palette, hash, name))
}

pub(super) fn color(palette: &Palette, hash: bool, name: &str) -> String {
    let (v1, v2, v3) = if hash {
        let name_hash = namehash(name);
        let reverse_name_hash = namehash(&name.chars().rev().collect::<String>());

        (name_hash, reverse_name_hash, reverse_name_hash)
    } else {
        (rand::random(), rand::random(), rand::random())
    };

    color_from_palette(palette, name, v1, v2, v3)
}

pub(super) fn bgcolor_for(palette: &Palette) -> (&'static str, &'static str) {
    match palette {
        Palette::Hot | Palette::Java | Palette::Js | Palette::Perl => YELLOW_GRADIENT,
        Palette::Mem | Palette::Chain => BLUE_GRADIENT,
        _ => GRAY_GRADIENT
    }
}

pub(super) fn read_palette(file: &str) -> io::Result<HashMap<String, String>> {
    let mut map = HashMap::default();
    let path = Path::new(file);

    // If the file does not exist, it is probably the first call to flamegraph with a consistent
    // palette: there is nothing to load.
    if path.exists() {
        let file = File::open(path)?;
        let file = BufReader::new(file);

        for line in file.lines() {
            let line = line?;
            let words = line.split("->").collect::<Vec<_>>();
            map.insert(words[0].to_string(), words[1].to_string());
        }
    }

    Ok(map)
}

pub(super) fn write_palette(file: &str, palette_map: HashMap<String, String>) -> Result<(), io::Error> {
    let mut file = OpenOptions::new().write(true).create(true).open(file)?;
    let mut entries = palette_map.into_iter().collect::<Vec<_>>();
    entries.sort_unstable();

    for (name, color) in entries {
        file.write_all(format!("{}->{}\n", name, color.to_string()).as_bytes())?
    }

    Ok(())
}
