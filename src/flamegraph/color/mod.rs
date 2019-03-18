//! Color palettes and options for flame graph generation.

use rand::rngs::ThreadRng;
use rand::Rng;
use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

mod palette_map;
mod palettes;

pub use palette_map::PaletteMap;
use rgb::RGB8;

/// A re-export of `RGB8` from the [`rgb` crate](https://docs.rs/rgb).
pub type Color = RGB8;

pub(super) const VDGREY: Color = Color {
    r: 160,
    g: 160,
    b: 160,
};
pub(super) const DGREY: Color = Color {
    r: 200,
    g: 200,
    b: 200,
};

const YELLOW_GRADIENT: (&str, &str) = ("#eeeeee", "#eeeeb0");
const BLUE_GRADIENT: (&str, &str) = ("#eeeeee", "#e0e0ff");
const GREEN_GRADIENT: (&str, &str) = ("#eef2ee", "#e0ffe0");
const GRAY_GRADIENT: (&str, &str) = ("#f8f8f8", "#e8e8e8");

/// A flame graph background color.
///
/// The default background color usually depends on the color scheme:
///
///  - [`BasicPalette::Mem`] defaults to [`BackgroundColor::Green`].
///  - [`BasicPalette::Io`] and [`MultiPalette::Wakeup`] default to [`BackgroundColor::Blue`].
///  - [`BasicPalette::Hot`] defaults to [`BackgroundColor::Yellow`].
///  - All other [`BasicPalette`] variants default to [`BackgroundColor::Grey`].
///  - All other [`MultiPalette`] variants default to [`BackgroundColor::Yellow`].
///
/// `BackgroundColor::default()` is `Yellow`.
#[derive(Clone, Copy, Debug)]
pub enum BackgroundColor {
    /// A yellow gradient from `#EEEEEE` to `#EEEEB0`.
    Yellow,
    /// A blue gradient from `#EEEEEE` to `#E0E0FF`.
    Blue,
    /// A green gradient from `#EEF2EE` to `#E0FFE0`.
    Green,
    /// A grey gradient from `#F8F8F8` to `#E8E8E8`.
    Grey,
    /// A flag background color with the given RGB components.
    ///
    /// Expressed in string form as `#RRGGBB` where each component is written in hexadecimal.
    Flat(Color),
}

impl Default for BackgroundColor {
    fn default() -> Self {
        BackgroundColor::Yellow
    }
}

/// A flame graph color palette.
///
/// Defaults to [`BasicPalette::Hot`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Palette {
    /// A plain color palette in which the color is not chosen based on function semantics.
    ///
    /// See [`BasicPalette`] for details.
    Basic(BasicPalette),
    /// A semantic color palette in which different hues are used to signifiy semantic aspects of
    /// different function names (kernel functions, JIT functions, etc.).
    Multi(MultiPalette),
}

impl Default for Palette {
    fn default() -> Self {
        Palette::Basic(BasicPalette::Hot)
    }
}

/// A plain color palette in which the color is not chosen based on function semantics.
///
/// Exactly how the color is chosen depends on a number of other configuration options like
/// [`super::Options.consistent_palette`] and [`super::Options.hash`]. In the absence of options
/// like that, these palettes all choose colors randomly from the indicated spectrum, and does not
/// consider the name of the frame's function when doing so.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BasicPalette {
    /// A palette in which colors are chosen from a red-yellow spectrum.
    Hot,
    /// A palette in which colors are chosen from a green-blue spectrum.
    Mem,
    /// A palette in which colors are chosen from a wide blue spectrum.
    Io,
    /// A palette in which colors are chosen from a red spectrum.
    Red,
    /// A palette in which colors are chosen from a green spectrum.
    Green,
    /// A palette in which colors are chosen from a blue spectrum.
    Blue,
    /// A palette in which colors are chosen from an aqua-tinted spectrum.
    Aqua,
    /// A palette in which colors are chosen from a yellow spectrum.
    Yellow,
    /// A palette in which colors are chosen from a purple spectrum.
    Purple,
    /// A palette in which colors are chosen from a orange spectrum.
    Orange,
}

/// A semantic color palette in which different hues are used to signifiy semantic aspects of
/// different function names (kernel functions, JIT functions, etc.).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MultiPalette {
    /// Use Java semantics to color frames.
    Java,
    /// Use JavaScript semantics to color frames.
    Js,
    /// Use Perl semantics to color frames.
    Perl,
    /// Equivalent to [`BasicPalette::Aqua`] with [`BackgroundColor::Blue`].
    Wakeup,
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
                .map(BackgroundColor::Flat)
                .ok_or_else(|| format!("unknown background color: {}", flat)),
        }
    }
}

macro_rules! u8_from_hex_iter {
    ($slice:expr) => {
        (($slice.next()?.to_digit(16)? as u8) << 4) | ($slice.next()?.to_digit(16)? as u8)
    };
}

fn parse_flat_bgcolor(s: &str) -> Option<Color> {
    if !s.starts_with('#') || (s.len() != 7) {
        None
    } else {
        let mut s = s[1..].chars();

        let r = u8_from_hex_iter!(s);
        let g = u8_from_hex_iter!(s);
        let b = u8_from_hex_iter!(s);

        Some(Color { r, g, b })
    }
}

/// `SearchColor::default()` is `rgb(230,0,230)`.
#[derive(Clone, Copy, Debug)]
pub struct SearchColor(Color);

impl Default for SearchColor {
    fn default() -> Self {
        SearchColor(Color {
            r: 230,
            g: 0,
            b: 230,
        })
    }
}

impl FromStr for SearchColor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_flat_bgcolor(s)
            .map(SearchColor)
            .ok_or_else(|| format!("unknown color: {}", s))
    }
}

impl fmt::Display for SearchColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rgb({},{},{})", self.0.r, self.0.g, self.0.b)
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

macro_rules! color {
    ($r:expr, $g:expr, $b:expr) => {
        Color {
            r: $r,
            g: $g,
            b: $b,
        }
    };
}

fn rgb_components_for_palette(palette: Palette, name: &str, v1: f32, v2: f32, v3: f32) -> Color {
    let basic_palette = match palette {
        Palette::Basic(basic) => basic,
        Palette::Multi(MultiPalette::Java) => palettes::java::resolve(name),
        Palette::Multi(MultiPalette::Perl) => palettes::perl::resolve(name),
        Palette::Multi(MultiPalette::Js) => palettes::js::resolve(name),
        Palette::Multi(MultiPalette::Wakeup) => palettes::wakeup::resolve(name),
    };

    match basic_palette {
        BasicPalette::Hot => color!(t!(205, 50, v3), t!(0, 230, v1), t!(0, 55, v2)),
        BasicPalette::Mem => color!(t!(0, 0, v3), t!(190, 50, v2), t!(0, 210, v1)),
        BasicPalette::Io => color!(t!(80, 60, v1), t!(80, 60, v1), t!(190, 55, v2)),
        BasicPalette::Red => color!(t!(200, 55, v1), t!(50, 80, v1), t!(50, 80, v1)),
        BasicPalette::Green => color!(t!(50, 60, v1), t!(200, 55, v1), t!(50, 60, v1)),
        BasicPalette::Blue => color!(t!(80, 60, v1), t!(80, 60, v1), t!(205, 50, v1)),
        BasicPalette::Yellow => color!(t!(175, 55, v1), t!(175, 55, v1), t!(50, 20, v1)),
        BasicPalette::Purple => color!(t!(190, 65, v1), t!(80, 60, v1), t!(190, 65, v1)),
        BasicPalette::Aqua => color!(t!(50, 60, v1), t!(165, 55, v1), t!(165, 55, v1)),
        BasicPalette::Orange => color!(t!(190, 65, v1), t!(90, 65, v1), t!(0, 0, v1)),
    }
}

pub(super) fn color(palette: Palette, hash: bool, name: &str, thread_rng: &mut ThreadRng) -> Color {
    let (v1, v2, v3) = if hash {
        let name_hash = namehash(name.bytes());
        let reverse_name_hash = namehash(name.bytes().rev());

        (name_hash, reverse_name_hash, reverse_name_hash)
    } else {
        (thread_rng.gen(), thread_rng.gen(), thread_rng.gen())
    };

    rgb_components_for_palette(palette, name, v1, v2, v3)
}

pub(super) fn color_scale(value: isize, max: usize) -> Color {
    if value == 0 {
        Color {
            r: 255,
            g: 255,
            b: 255,
        }
    } else if value > 0 {
        // A positive value indicates _more_ samples,
        // and hence more time spent, so we give it a red hue.
        let c = (210 * (max as isize - value) / max as isize) as u8;
        Color { r: 255, g: c, b: c }
    } else {
        // A negative value indicates _fewer_ samples,
        // or a speed-up, so we give it a green hue.
        let c = (210 * (max as isize + value) / max as isize) as u8;
        Color { r: c, g: c, b: 255 }
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
        BackgroundColor::Flat(color) => {
            let color = format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
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
    use super::Color;

    #[test]
    fn bgcolor_parse_test() {
        assert_eq!(
            parse_flat_bgcolor("#ffffff"),
            Some(color!(0xff, 0xff, 0xff))
        );
        assert_eq!(
            parse_flat_bgcolor("#000000"),
            Some(color!(0x00, 0x00, 0x00))
        );
        assert_eq!(
            parse_flat_bgcolor("#abcdef"),
            Some(color!(0xab, 0xcd, 0xef))
        );
        assert_eq!(
            parse_flat_bgcolor("#123456"),
            Some(color!(0x12, 0x34, 0x56))
        );
        assert_eq!(
            parse_flat_bgcolor("#789000"),
            Some(color!(0x78, 0x90, 0x00))
        );
        assert_eq!(parse_flat_bgcolor("ffffff"), None);
        assert_eq!(parse_flat_bgcolor("#fffffff"), None);
        assert_eq!(parse_flat_bgcolor("#xfffff"), None);
        assert_eq!(parse_flat_bgcolor("# fffff"), None);
    }

    macro_rules! test_hash {
        ($name:expr, $expected:expr) => {
            assert!((dbg!(namehash($name.bytes())) - $expected).abs() < std::f32::EPSILON);
        };
    }

    #[test]
    fn namehash_test() {
        test_hash!(
            "org/mozilla/javascript/NativeFunction:.initScriptFunction_[j]",
            0.779_646_04
        );
        test_hash!(
            "]j[_noitcnuFtpircStini.:noitcnuFevitaN/tpircsavaj/allizom/gro",
            0.644_153_1
        );
        test_hash!("genunix`kmem_cache_free", 0.466_926_34);
        test_hash!("eerf_ehcac_memk`xinuneg", 0.840_410_3);
        test_hash!("unix`0xfffffffffb8001d6", 0.418_131_17);
        test_hash!("6d1008bfffffffffx0`xinu", 0.840_410_3);
        test_hash!("un`0xfffffffffb8001d6", 0.418_131_17);
        test_hash!("``0xfffffffffb8001d6", 0.418_131_17);
        test_hash!("", 1.0);
    }
}
