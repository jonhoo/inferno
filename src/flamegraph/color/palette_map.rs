use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::str::FromStr;

#[derive(Default)]
pub struct PaletteMap<'a>(HashMap<Cow<'a, str>, (u8, u8, u8)>);

impl<'a> PaletteMap<'a> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get(&self, func: &str) -> Option<(u8, u8, u8)> {
        self.0.get(func).cloned()
    }

    pub fn insert(&mut self, func: &'a str, color: (u8, u8, u8)) -> Option<(u8, u8, u8)> {
        self.0.insert(Cow::from(func), color)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, (u8, u8, u8))> {
        self.0.iter().map(|(func, color)| (func.as_ref(), *color))
    }

    pub fn from_stream(reader: &mut dyn io::Read) -> io::Result<Self> {
        let mut map = HashMap::default();
        let reader = BufReader::new(reader);

        for line in reader.lines() {
            let line = line?;

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

            let rgb_color = parse_rgb_string(color)
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
            map.insert(Cow::from(name.to_string()), rgb_color);
        }

        Ok(PaletteMap(map))
    }

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

    pub fn load_from_file(path: &dyn AsRef<Path>) -> io::Result<Self> {
        // If the file does not exist, it is probably the first call to flamegraph with a consistent
        // palette: there is nothing to load.
        if path.exists() {
            let mut file = File::open(path)?;
            PaletteMap::from_stream(&mut file)
        } else {
            Ok(PaletteMap::default())
        }
    }

    pub fn save_to_file(&self, path: &dyn AsRef<Path>) -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create(true).open(path)?;
        self.to_stream(&mut file)
    }

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
