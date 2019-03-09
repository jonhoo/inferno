use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::str::FromStr;

/// Mapping of the association between a function name and the color used when drawing information
/// from this function.
#[derive(Default)]
pub struct PaletteMap<'a>(HashMap<Cow<'a, str>, (u8, u8, u8)>);

impl<'a> PaletteMap<'a> {
    /// Returns the color value corresponding to the given function name.
    pub fn get(&self, func: &str) -> Option<(u8, u8, u8)> {
        self.0.get(func).cloned()
    }

    /// Inserts a function name/color pair in the map.
    pub fn insert(&mut self, func: &'a str, color: (u8, u8, u8)) -> Option<(u8, u8, u8)> {
        self.0.insert(Cow::from(func), color)
    }

    /// Provides an iterator over the elements of the map.
    pub fn iter(&self) -> impl Iterator<Item = (&str, (u8, u8, u8))> {
        self.0.iter().map(|(func, color)| (func.as_ref(), *color))
    }

    /// Builds a mapping based on the inputs given by the reader.
    ///
    /// The reader should provide name/color pairs as text input, each pair separated by a line
    /// separator.
    ///
    /// Each line should follow the format: NAME->rgb(RED, GREEN, BLUE)
    /// where NAME is the function name, and RED, GREEN, BLUE integer values between 0 and 255
    /// included.
    ///
    /// This function will return an [`std::io::Error`] if the input is not correctly formatted.
    pub fn from_stream(reader: &mut dyn io::Read) -> io::Result<Self> {
        let mut map = HashMap::default();
        let reader = BufReader::new(reader);

        for line in reader.lines() {
            let line = line?;
            let (name, color) = parse_line(&line)?;
            map.insert(Cow::from(name.to_string()), color);
        }

        Ok(PaletteMap(map))
    }

    /// Writes the palette map using the given writer.
    /// The output content will follow the same format described in [from_stream()]
    /// The name/color pairs will be sorted, based on the name lexicographic order.
    pub fn to_stream(&self, writer: &mut dyn io::Write) -> io::Result<()> {
        let mut entries = self.0.iter().collect::<Vec<_>>();
        // We sort the palette because the Perl implementation does.
        entries.sort_unstable();

        for (name, color) in entries {
            writer.write_all(
                format!("{}->rgb({},{},{})\n", name, color.0, color.1, color.2).as_bytes(),
            )?
        }

        Ok(())
    }

    /// Utility function to load a palette map from a file.
    ///
    /// The file content should follow the format described in [from_stream()].
    ///
    /// If the file does not exist, and empty palette map is returned.
    pub fn load_from_file(path: &dyn AsRef<Path>) -> io::Result<Self> {
        // If the file does not exist, it is probably the first call to flamegraph with a consistent
        // palette: there is nothing to load.
        if path.as_ref().exists() {
            let mut file = File::open(path)?;
            PaletteMap::from_stream(&mut file)
        } else {
            Ok(PaletteMap::default())
        }
    }

    /// Utility function to save a palette map to a file.
    ///
    /// The file content will follow the format described in [from_stream()].
    pub fn save_to_file(&self, path: &dyn AsRef<Path>) -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create(true).open(path)?;
        self.to_stream(&mut file)
    }

    /// Returns the color value corresponding to the given function name if it is present.
    /// Otherwise compute the color, and insert the new function name/color in the map.
    pub fn find_color_for<F: FnMut(&'a str) -> (u8, u8, u8)>(
        &mut self,
        name: &'a str,
        mut compute_color: F,
    ) -> (u8, u8, u8) {
        *self
            .0
            .entry(Cow::from(name))
            .or_insert_with(|| compute_color(name))
    }
}

fn parse_line(line: &str) -> io::Result<(&str, (u8, u8, u8))> {
    // A line is formatted like this: NAME -> rbg(RED, GREEN, BLUE)
    let mut words = line.split("->");

    let name = match words.next() {
        Some(name) => name,
        None => return Err(io::Error::from(io::ErrorKind::InvalidInput)),
    };

    let color = match words.next() {
        Some(name) => name,
        None => return Err(io::Error::from(io::ErrorKind::InvalidInput)),
    };

    if words.next().is_some() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }

    let rgb_color =
        parse_rgb_string(color).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;

    Ok((name, rgb_color))
}

fn parse_rgb_string(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();

    if !s.starts_with("rgb(") || !s.ends_with(')') {
        return None;
    }

    let s = &s["rgb(".len()..s.len() - 1];
    let r_end_index = s.find(',')?;
    let r_str = s[..r_end_index].trim();
    let r = u8::from_str(r_str).ok()?;

    let s = &s[r_end_index + 1..];
    let g_end_index = s.find(',')?;
    let g_str = s[..g_end_index].trim();
    let g = u8::from_str(g_str).ok()?;

    let b_str = &s[g_end_index + 1..].trim();
    let b = u8::from_str(b_str).ok()?;

    Some((r, g, b))
}

#[cfg(test)]
mod tests {
    use crate::flamegraph::color::palette_map::{parse_line, PaletteMap};

    #[test]
    fn palette_map_test() {
        let mut palette = PaletteMap::default();

        assert_eq!(palette.insert("foo", (0, 50, 255)), None);
        assert_eq!(palette.insert("bar", (50, 0, 60)), None);
        assert_eq!(palette.insert("foo", (80, 20, 63)), Some((0, 50, 255)));
        assert_eq!(palette.insert("foo", (128, 128, 128)), Some((80, 20, 63)));
        assert_eq!(palette.insert("baz", (255, 0, 255)), None);

        assert_eq!(palette.get("func"), None);
        assert_eq!(palette.get("bar"), Some((50, 0, 60)));
        assert_eq!(palette.get("foo"), Some((128, 128, 128)));
        assert_eq!(palette.get("baz"), Some((255, 0, 255)));

        let mut vec = palette.iter().collect::<Vec<_>>();
        vec.sort_unstable();
        let mut iter = vec.iter();

        assert_eq!(iter.next(), Some(&("bar", (50, 0, 60))));
        assert_eq!(iter.next(), Some(&("baz", (255, 0, 255))));
        assert_eq!(iter.next(), Some(&("foo", (128, 128, 128))));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn parse_line_test() {
        assert_eq!(
            parse_line("func->rgb(0, 0, 0)").unwrap(),
            ("func", (0, 0, 0))
        );
        assert_eq!(
            parse_line("->rgb(255, 255, 255)").unwrap(),
            ("", (255, 255, 255))
        );

        assert!(parse_line("").is_err());
        assert!(parse_line("func->(0, 0, 0)").is_err());
        assert!(parse_line("func->").is_err());
        assert!(parse_line("func->foo->rgb(0, 0, 0)").is_err());
        assert!(parse_line("func->rgb(0, 0, 0)->foo").is_err());
        assert!(parse_line("func->rgb(255, 255, 256)").is_err());
        assert!(parse_line("func->rgb(-1, 255, 255)").is_err());
    }
}
