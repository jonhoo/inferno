use std::collections::HashMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::path::Path;

pub fn read_palette(file: &str) -> io::Result<HashMap<String, String>> {
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

pub fn write_palette(file: &str, palette_map: HashMap<String, String>) -> Result<(), io::Error> {
    let mut file = OpenOptions::new().write(true).create(true).open(file)?;
    let mut entries = palette_map.into_iter().collect::<Vec<_>>();
    // We sort the palette because the Perl implementation does.
    entries.sort_unstable();

    for (name, color) in entries {
        file.write_all(format!("{}->{}\n", name, color.to_string()).as_bytes())?
    }

    Ok(())
}
