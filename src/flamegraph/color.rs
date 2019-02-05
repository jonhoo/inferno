use crate::flamegraph::palettes;
use rand::rngs::ThreadRng;
use rand::Rng;
use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

pub(super) const VDGREY: &str = "rgb(160,160,160)";
pub(super) const DGREY: &str = "rgb(200,200,200)";

const YELLOW_GRADIENT: (&str, &str) = ("#eeeeee", "#eeeeb0");
const BLUE_GRADIENT: (&str, &str) = ("#eeeeee", "#e0e0ff");
const GRAY_GRADIENT: (&str, &str) = ("#f8f8f8", "#e8e8e8");

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Palette {
    Basic(BasicPalette),
    Multi(MultiPalette),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BasicPalette {
    Hot,
    Mem,
    Io,
    Red,
    Green,
    Blue,
    Aqua,
    Yellow,
    Purple,
    Orange,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MultiPalette {
    Java,
    Js,
    Perl,
    Wakeup,
}

impl Default for Palette {
    fn default() -> Self {
        Palette::Basic(BasicPalette::Hot)
    }
}

impl FromStr for Palette {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "hot" => Ok(Palette::Basic(BasicPalette::Hot)),
            "mem" => Ok(Palette::Basic(BasicPalette::Mem)),
            "io" => Ok(Palette::Basic(BasicPalette::Io)),
            "wakeup" => Ok(Palette::Multi(MultiPalette::Wakeup)),
            "java" => Ok(Palette::Multi(MultiPalette::Java)),
            "js" => Ok(Palette::Multi(MultiPalette::Js)),
            "perl" => Ok(Palette::Multi(MultiPalette::Perl)),
            "red" => Ok(Palette::Basic(BasicPalette::Red)),
            "green" => Ok(Palette::Basic(BasicPalette::Green)),
            "blue" => Ok(Palette::Basic(BasicPalette::Blue)),
            "aqua" => Ok(Palette::Basic(BasicPalette::Aqua)),
            "yellow" => Ok(Palette::Basic(BasicPalette::Yellow)),
            "purple" => Ok(Palette::Basic(BasicPalette::Purple)),
            "orange" => Ok(Palette::Basic(BasicPalette::Orange)),
            unknown => Err(format!("unknown color palette: {}", unknown)),
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
        let looking_for_module_name = if name.starts_with('`') {
            &name[1..]
        } else {
            name
        };

        if let Some(index) = looking_for_module_name.find('`') {
            &looking_for_module_name[index + 1..]
        } else {
            name
        }
    };

    // The Perl implementation does a check for modulo > 12,
    // but that's equivalent to just iterating over the first three characters
    // (as long as modulo starts equal to 10)
    for character in name.bytes().take(3) {
        let i = f32::from(character % modulo);
        vector += (i / f32::from(modulo - 1)) * weight;
        modulo += 1;
        max += weight;
        weight *= 0.70;
    }

    (1.0 - vector / max)
}

macro_rules! t {
    ($b:expr, $a:expr, $x:expr) => {
        $b + ($a as f32 * $x) as u8
    };
}

fn rgb_components_for_palette(
    palette: Palette,
    name: &str,
    v1: f32,
    v2: f32,
    v3: f32,
) -> (u8, u8, u8) {
    let basic_palette = match palette {
        Palette::Basic(basic) => basic,
        Palette::Multi(MultiPalette::Java) => palettes::java::resolve(name),
        Palette::Multi(MultiPalette::Perl) => palettes::perl::resolve(name),
        Palette::Multi(MultiPalette::Js) => palettes::js::resolve(name),
        Palette::Multi(MultiPalette::Wakeup) => palettes::wakeup::resolve(name),
    };

    match basic_palette {
        BasicPalette::Hot => (t!(205, 50, v3), t!(0, 230, v1), t!(0, 55, v2)),
        BasicPalette::Mem => (t!(0, 0, v3), t!(190, 50, v2), t!(0, 210, v1)),
        BasicPalette::Io => (t!(80, 60, v1), t!(80, 60, v1), t!(190, 55, v2)),
        BasicPalette::Red => (t!(200, 55, v1), t!(50, 80, v1), t!(50, 80, v1)),
        BasicPalette::Green => (t!(50, 60, v1), t!(200, 55, v1), t!(50, 60, v1)),
        BasicPalette::Blue => (t!(80, 60, v1), t!(80, 60, v1), t!(205, 50, v1)),
        BasicPalette::Yellow => (t!(175, 55, v1), t!(175, 55, v1), t!(50, 20, v1)),
        BasicPalette::Purple => (t!(190, 65, v1), t!(80, 60, v1), t!(190, 65, v1)),
        BasicPalette::Aqua => (t!(50, 60, v1), t!(165, 55, v1), t!(165, 55, v1)),
        BasicPalette::Orange => (t!(190, 65, v1), t!(90, 65, v1), t!(0, 0, v1)),
    }
}

fn color_from_palette(palette: Palette, name: &str, v1: f32, v2: f32, v3: f32) -> String {
    let (red, green, blue) = rgb_components_for_palette(palette, name, v1, v2, v3);

    format!("rgb({},{},{})", red, green, blue)
}

pub(super) fn color_map<'a>(
    palette: Palette,
    hash: bool,
    name: &'a str,
    palette_map: &'a mut HashMap<String, String>,
    thread_rng: &mut ThreadRng,
) -> &'a str {
    palette_map
        .entry(name.to_string())
        .or_insert_with(|| color(palette, hash, name, thread_rng))
}

pub(super) fn color(
    palette: Palette,
    hash: bool,
    name: &str,
    thread_rng: &mut ThreadRng,
) -> String {
    let (v1, v2, v3) = if hash {
        let name_hash = namehash(name);
        let reverse_name_hash = namehash(&name.chars().rev().collect::<String>());

        (name_hash, reverse_name_hash, reverse_name_hash)
    } else {
        (thread_rng.gen(), thread_rng.gen(), thread_rng.gen())
    };

    color_from_palette(palette, name, v1, v2, v3)
}

pub(super) fn bgcolor_for(palette: Palette) -> (&'static str, &'static str) {
    match palette {
        Palette::Basic(BasicPalette::Hot)
        | Palette::Multi(MultiPalette::Java)
        | Palette::Multi(MultiPalette::Js)
        | Palette::Multi(MultiPalette::Perl) => YELLOW_GRADIENT,
        Palette::Basic(BasicPalette::Mem) => BLUE_GRADIENT,
        _ => GRAY_GRADIENT,
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

pub(super) fn write_palette(
    file: &str,
    palette_map: HashMap<String, String>,
) -> Result<(), io::Error> {
    let mut file = OpenOptions::new().write(true).create(true).open(file)?;
    let mut entries = palette_map.into_iter().collect::<Vec<_>>();
    // We sort the palette because the Perl implementation does.
    entries.sort_unstable();

    for (name, color) in entries {
        file.write_all(format!("{}->{}\n", name, color.to_string()).as_bytes())?
    }

    Ok(())
}
