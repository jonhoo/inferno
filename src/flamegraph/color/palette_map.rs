use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

pub struct PaletteMap(HashMap<String, (u8, u8, u8)>);

impl PaletteMap {
    pub fn load(file: &str) -> quick_xml::Result<Self> {
        let mut map = HashMap::default();
        let path = Path::new(file);

        // If the file does not exist, it is probably the first call to flamegraph with a consistent
        // palette: there is nothing to load.
        if path.exists() {
            let file = File::open(path).map_err(quick_xml::Error::Io)?;
            let file = BufReader::new(file);

            for line in file.lines() {
                let line = line.map_err(quick_xml::Error::Io)?;
                let words = line.split("->").collect::<Vec<_>>();
                let rgb_color = parse_rgb_string(words[1])
                    .ok_or_else(|| quick_xml::Error::UnexpectedToken(words[1].to_string()))?;
                map.insert(words[0].to_string(), rgb_color);
            }
        }

        Ok(PaletteMap(map))
    }

    pub fn save(self, file: &str) -> Result<(), io::Error> {
        let mut file = OpenOptions::new().write(true).create(true).open(file)?;
        let mut entries = self.0.into_iter().collect::<Vec<_>>();
        // We sort the palette because the Perl implementation does.
        entries.sort_unstable();

        for (name, color) in entries {
            file.write_all(
                format!("{}->rgb({},{},{})\n", name, color.0, color.1, color.2).as_bytes(),
            )?
        }

        Ok(())
    }

    pub fn find_color_for<'a, F: FnMut(&'a str) -> (u8, u8, u8)>(
        &'a mut self,
        name: &'a str,
        mut compute_color: F,
    ) -> (u8, u8, u8) {
        *self
            .0
            .entry(name.to_string())
            .or_insert_with(|| compute_color(name))
    }
}

fn parse_rgb_string(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();

    if !s.starts_with("rgb(") || !s.ends_with(')') {
        return None;
    }

    let string_end = s.len() - 1;

    let r_start = "rgb(".len();
    let r_end_index = &s[r_start..string_end].find(',')?;
    let r_str = s[..r_end_index - 1].trim();
    let r = u8::from_str(r_str).ok()?;

    let g_end_index = &s[r_end_index + 1..string_end].find(',')?;
    let g_str = s[..g_end_index - 1].trim();
    let g = u8::from_str(g_str).ok()?;

    let b_str = s[g_end_index + 1..string_end].trim();
    let b = u8::from_str(b_str).ok()?;

    Some((r, g, b))
}
