use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::path::Path;

pub struct PaletteMap(HashMap<String, String>);

impl PaletteMap {
    pub fn load(file: &str) -> io::Result<Self> {
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

        Ok(PaletteMap(map))
    }

    pub fn save(self, file: &str) -> Result<(), io::Error> {
        let mut file = OpenOptions::new().write(true).create(true).open(file)?;
        let mut entries = self.0.into_iter().collect::<Vec<_>>();
        // We sort the palette because the Perl implementation does.
        entries.sort_unstable();

        for (name, color) in entries {
            file.write_all(format!("{}->{}\n", name, color.to_string()).as_bytes())?
        }

        Ok(())
    }

    pub fn find_color_for<'a, F: FnMut(&'a str) -> String>(
        &'a mut self,
        name: &'a str,
        mut compute_color: F,
    ) -> &'a str {
        self.0
            .entry(name.to_string())
            .or_insert_with(|| compute_color(name))
    }
}
