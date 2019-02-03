use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Error;
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

struct RGB {
    red: u8,
    green: u8,
    blue: u8,
}

impl Display for RGB {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "rbg({},{},{})", self.red, self.green, self.blue)
    }
}

fn namehash(name: &str) -> f64 {
    let mut vector = 0f64;
    let mut weight = 1f64;
    let mut max = 1f64;
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

    for character in name.bytes() {
        let i = (character % modulo) as f64;
        vector += (i / ((modulo - 1) as f64)) * weight;
        modulo += 1;
        max += weight;
        weight *= 0.70;

        if modulo > 12 {
            break
        }
    }

    (1f64 - vector / max)
}

fn handle_java_palette(s: &str) -> Palette {
    if s.ends_with("]") {
        if let Some(ai) = s.rfind("_[") {
            if s[ai..].len() == 4 {
                let suffix_char = &s[ai+2..ai+3];
                if suffix_char.starts_with("k") { return Palette::Orange }
                if suffix_char.starts_with("i") { return Palette::Aqua }
                if suffix_char.starts_with("j") { return Palette::Green }
            }
        }
    }

    let java_prefix = if s.starts_with("L") { &s[1..] } else { s };

    if java_prefix.starts_with("java") ||
        java_prefix.starts_with("org") ||
        java_prefix.starts_with("com") ||
        java_prefix.starts_with("io") ||
        java_prefix.starts_with("sun") {
        Palette::Green
    } else if s.contains("::") {
        Palette::Yellow
    } else {
        Palette::Red
    }
}

fn handle_perl_palette(s: &str) -> Palette {
    if s.contains("::") {
        Palette::Yellow
    } else if s.contains("Perl") || s.contains(".pl") {
        Palette::Green
    } else if s.ends_with("_[k]") {
        Palette::Orange
    } else {
        Palette::Red
    }
}

fn handle_js_palette(s: &str) -> Palette {
    if s.ends_with("_[j]") {
        if s.contains("/") {
            return Palette::Green
        } else {
            return Palette::Aqua
        }
    } else if s.contains("::") {
        return Palette::Yellow
    } else if let Some(ai) = s.find("/") {
        if (&s[ai..]).contains(".js") {
            return Palette::Green
        }
    }

    if s.contains(":") {
        Palette::Aqua
    } else if s == " " {
        Palette::Green
    } else if s.contains("_[k]") {
        Palette::Orange
    } else {
        Palette::Red
    }
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

fn coefficients_for_palette(palette: &Palette, name: &str, v1: f64, v2: f64, v3: f64) ->
    ((u8, u8, f64), (u8, u8, f64), (u8, u8, f64)) {
    let real_palette = match palette {
        Palette::Hot => return ((205, 50, v3), (0, 230, v1), (0, 55, v2)),
        Palette::Mem => return ((0, 0, v3), (190, 50, v2), (0, 210, v1)),
        Palette::Io => return ((80, 60, v1), (80, 60, v1), (190, 55, v2)),
        Palette::Red => return ((200, 55, v1), (50, 80, v1), (50, 80, v1)),
        Palette::Green => return ((50, 60, v1), (200, 55, v1), (50, 60, v1)),
        Palette::Blue => return ((80, 60, v1), (80, 60, v1), (205, 60, v1)),
        Palette::Yellow => return ((175, 55, v1), (175, 55, v1), (50, 20, v1)),
        Palette::Purple => return ((190, 65, v1), (80, 60, v1), (190, 65, v1)),
        Palette::Aqua => return ((50, 60, v1), (165, 55, v1), (165, 55, v1)),
        Palette::Orange => return ((190, 65, v1), (90, 65, v1), (0, 0, v1)),
        Palette::Java => handle_java_palette(name),
        Palette::Perl => handle_perl_palette(name),
        Palette::Js => handle_js_palette(name),
        Palette::Wakeup => handle_wakeup_palette(name),
        Palette::Chain => handle_chain_palette(name),
    };

    coefficients_for_palette(&real_palette, name, v1, v2, v3)
}

fn affine_transform(a: u8, b: u8, x: f64) -> u8 {
    b + (a as f64 * x) as u8
}

fn color_from_palette(palette: &Palette, name: &str, v1: f64, v2: f64, v3: f64) -> RGB {
    let ((r_b, r_a, r_x),
         (g_b, g_a, g_x),
         (b_b, b_a, b_x)) = coefficients_for_palette(palette, name, v1, v2, v3);

    RGB {
        red: affine_transform(r_a, r_b, r_x),
        green: affine_transform(g_a, g_b, g_x),
        blue: affine_transform(b_a, b_b, b_x),
    }
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

    color_from_palette(palette, name, v1, v2, v3).to_string()
}

pub(super) fn get_background_colors_for(palette: &Palette) -> (&str, &str) {
    match palette {
        Palette::Hot | Palette::Java | Palette::Js | Palette::Perl => {
            YELLOW_GRADIENT
        },
        Palette::Mem | Palette::Chain => {
            BLUE_GRADIENT
        },
        _ => {
            GRAY_GRADIENT
        }
    }
}

pub(super) fn read_palette(file: &str) -> Result<HashMap<String, String>, io::Error> {
    let mut map = HashMap::default();
    let path = Path::new(file);

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
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    for (name, color) in entries {
        file.write_all(format!("{}->{}\n", name, color.to_string()).as_bytes())?
    }

    Ok(())
}