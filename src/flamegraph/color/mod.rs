use rand::rngs::ThreadRng;
use rand::Rng;
use std::borrow::Cow;
use std::str::FromStr;

mod palette_map;
mod palettes;

pub(super) use palette_map::PaletteMap;

pub(super) const VDGREY: (u8, u8, u8) = (160, 160, 160);
pub(super) const DGREY: (u8, u8, u8) = (200, 200, 200);

const YELLOW_GRADIENT: (&str, &str) = ("#eeeeee", "#eeeeb0");
const BLUE_GRADIENT: (&str, &str) = ("#eeeeee", "#e0e0ff");
const GREEN_GRADIENT: (&str, &str) = ("#eef2ee", "#e0ffe0");
const GRAY_GRADIENT: (&str, &str) = ("#f8f8f8", "#e8e8e8");

#[derive(Clone, Copy, Debug)]
pub enum BackgroundColor {
    Yellow,
    Blue,
    Green,
    Grey,
    Flat(u8, u8, u8),
}

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

impl Default for BackgroundColor {
    fn default() -> Self {
        BackgroundColor::Yellow
    }
}

impl FromStr for BackgroundColor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "yellow" => Ok(BackgroundColor::Yellow),
            "blue" => Ok(BackgroundColor::Blue),
            "green" => Ok(BackgroundColor::Green),
            "grey" => Ok(BackgroundColor::Grey),
            flat => parse_flat_bgcolor(flat)
                .map(|(r, g, b)| BackgroundColor::Flat(r, g, b))
                .ok_or_else(|| format!("unknown background color: {}", flat)),
        }
    }
}

macro_rules! u8_from_hex_iter {
    ($slice:expr) => {
        (($slice.next()?.to_digit(16)? as u8) << 4) | ($slice.next()?.to_digit(16)? as u8)
    };
}

fn parse_flat_bgcolor(s: &str) -> Option<(u8, u8, u8)> {
    if !s.starts_with('#') || (s.len() != 7) {
        None
    } else {
        let mut s = s[1..].chars();

        let r = u8_from_hex_iter!(s);
        let g = u8_from_hex_iter!(s);
        let b = u8_from_hex_iter!(s);

        Some((r, g, b))
    }
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

struct NamehashVariables {
    vector: f32,
    weight: f32,
    max: f32,
    modulo: u8,
}

impl NamehashVariables {
    fn init() -> Self {
        NamehashVariables {
            vector: 0.0,
            weight: 1.0,
            max: 1.0,
            modulo: 10,
        }
    }

    fn update(&mut self, character: u8) {
        let i = f32::from(character % self.modulo);
        self.vector += (i / f32::from(self.modulo - 1)) * self.weight;
        self.modulo += 1;
        self.max += self.weight;
        self.weight *= 0.70;
    }

    fn result(&self) -> f32 {
        (1.0 - self.vector / self.max)
    }
}

/// Generate a vector hash for the name string, weighting early over
/// later characters. We want to pick the same colors for function
/// names across different flame graphs.
fn namehash<I: Iterator<Item = u8>>(mut name: I) -> f32 {
    let mut namehash_variables = NamehashVariables::init();
    let mut module_name_found = false;

    // The original Perl regex is: $name =~ s/.(.*?)`//;
    // Ie. we want to remove everything before the first '`'. If '`' is the first character,
    // we remove everything before the second '`'. If there is no '`', we keep everything.
    // This becomes tricky because we want to compute the hash and do the potential deletion
    // ine one pass only.
    // So, we start computing the hash and we check for '`' after the first character.
    // If we find '`' before the end of the computation (3 characters), we stop the computation.
    // If the computation finishes normally, we search for the first next '`'.
    // After that, either we have found a '`' (end of prefix), and we need to compute the hash from there,
    // or there is no '`' in the iterator and we have the hash computed!
    // In the Perl implementation, the hash was computed while `modulo > 12`, which means 3 iterations
    // maximum because modulo is initialized at 10.

    match name.next() {
        None => return namehash_variables.result(),
        Some(first_char) => namehash_variables.update(first_char),
    }

    for character in name.by_ref().take(2) {
        if character == b'`' {
            module_name_found = true;
            break;
        }

        namehash_variables.update(character);
    }

    module_name_found = module_name_found || name.any(|c| c == b'`');

    if module_name_found {
        namehash_variables = NamehashVariables::init();

        for character in name.take(3) {
            namehash_variables.update(character)
        }
    }

    namehash_variables.result()
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

pub(super) fn color(
    palette: Palette,
    hash: bool,
    name: &str,
    thread_rng: &mut ThreadRng,
) -> (u8, u8, u8) {
    let (v1, v2, v3) = if hash {
        let name_hash = namehash(name.bytes());
        let reverse_name_hash = namehash(name.bytes().rev());

        (name_hash, reverse_name_hash, reverse_name_hash)
    } else {
        (thread_rng.gen(), thread_rng.gen(), thread_rng.gen())
    };

    rgb_components_for_palette(palette, name, v1, v2, v3)
}

pub(super) fn color_scale(value: isize, max: usize) -> (u8, u8, u8) {
    if value == 0 {
        (255, 255, 255)
    } else if value > 0 {
        // A positive value indicates _more_ samples,
        // and hence more time spent, so we give it a red hue.
        let c = (210 * (max as isize - value) / max as isize) as u8;
        (255, c, c)
    } else {
        // A negative value indicates _fewer_ samples,
        // or a speed-up, so we give it a green hue.
        let c = (210 * (max as isize + value) / max as isize) as u8;
        (c, c, 255)
    }
}

fn default_bg_color_for(palette: Palette) -> BackgroundColor {
    match palette {
        Palette::Basic(BasicPalette::Mem) => BackgroundColor::Green,
        Palette::Basic(BasicPalette::Io) | Palette::Multi(MultiPalette::Wakeup) => {
            BackgroundColor::Blue
        }
        Palette::Basic(BasicPalette::Red)
        | Palette::Basic(BasicPalette::Green)
        | Palette::Basic(BasicPalette::Blue)
        | Palette::Basic(BasicPalette::Aqua)
        | Palette::Basic(BasicPalette::Yellow)
        | Palette::Basic(BasicPalette::Purple)
        | Palette::Basic(BasicPalette::Orange) => BackgroundColor::Grey,
        _ => BackgroundColor::Yellow,
    }
}

macro_rules! cow {
    ($gradient:expr) => {
        (Cow::from($gradient.0), Cow::from($gradient.1))
    };
}

pub(super) fn bgcolor_for<'a>(
    bgcolor: Option<BackgroundColor>,
    palette: Palette,
) -> (Cow<'a, str>, Cow<'a, str>) {
    let bgcolor = bgcolor.unwrap_or_else(|| default_bg_color_for(palette));

    match bgcolor {
        BackgroundColor::Yellow => cow!(YELLOW_GRADIENT),
        BackgroundColor::Blue => cow!(BLUE_GRADIENT),
        BackgroundColor::Green => cow!(GREEN_GRADIENT),
        BackgroundColor::Grey => cow!(GRAY_GRADIENT),
        BackgroundColor::Flat(r, g, b) => {
            let color = format!("#{:02x}{:02x}{:02x}", r, g, b);
            let first = Cow::from(color);
            let second = first.clone();
            (first, second)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::namehash;
    use super::parse_flat_bgcolor;

    #[test]
    fn bgcolor_parse_test() {
        assert_eq!(parse_flat_bgcolor("#ffffff"), Some((0xff, 0xff, 0xff)));
        assert_eq!(parse_flat_bgcolor("#000000"), Some((0x00, 0x00, 0x00)));
        assert_eq!(parse_flat_bgcolor("#abcdef"), Some((0xab, 0xcd, 0xef)));
        assert_eq!(parse_flat_bgcolor("#123456"), Some((0x12, 0x34, 0x56)));
        assert_eq!(parse_flat_bgcolor("#789000"), Some((0x78, 0x90, 0x00)));
        assert_eq!(parse_flat_bgcolor("ffffff"), None);
        assert_eq!(parse_flat_bgcolor("#fffffff"), None);
        assert_eq!(parse_flat_bgcolor("#xfffff"), None);
        assert_eq!(parse_flat_bgcolor("# fffff"), None);
    }

    macro_rules! test_hash {
        ($name:expr, $expected:expr) => {
            assert_eq!(namehash($name.bytes()), $expected)
        };
    }

    #[test]
    fn namehash_test() {
        test_hash!(
            "org/mozilla/javascript/NativeFunction:.initScriptFunction_[j]",
            0.77964604
        );
        test_hash!(
            "]j[_noitcnuFtpircStini.:noitcnuFevitaN/tpircsavaj/allizom/gro",
            0.64415313
        );
        test_hash!("genunix`kmem_cache_free", 0.46692634);
        test_hash!("eerf_ehcac_memk`xinuneg", 0.84041037);
        test_hash!("unix`0xfffffffffb8001d6", 0.41813117);
        test_hash!("6d1008bfffffffffx0`xinu", 0.84041037);
        test_hash!("un`0xfffffffffb8001d6", 0.41813117);
        test_hash!("``0xfffffffffb8001d6", 0.41813117);
        test_hash!("", 1.0);
    }
}
