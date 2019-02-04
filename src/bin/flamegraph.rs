use std::fs::File;
use std::io::{self, BufReader};
use structopt::StructOpt;

use inferno::flamegraph::{self, Options, color::Palette};

#[derive(Debug, StructOpt)]
#[structopt(name = "inferno-flamegraph", author = "")]
struct Opt {
    /// collapsed perf output file, or STDIN if not specified
    infile: Option<String>,
    /// set color palette
    #[structopt(short = "c", long = "colors", default_value = "hot", raw(possible_values =
    r#"&["hot","mem","io","wakeup","java","js","perl","red","green","blue","aqua","yellow","purple","orange"]"#))]
    colors: Palette,
    /// colors are keyed by function name hash
    #[structopt(long = "hash")]
    hash: bool,
    /// use consistent palette (palette.map)
    #[structopt(long = "cp")]
    cp: bool,
}

impl Into<Options> for Opt {
    fn into(self) -> Options {
        Options {
            colors: self.colors,
            hash: self.hash,
            consistent_palette: self.cp
        }
    }
}

fn main() -> quick_xml::Result<()> {
    let (infile, options) = {
        let opt = Opt::from_args();
        (opt.infile.clone(), opt.into())
    };

    match infile {
        Some(ref f) => {
            let r =
                BufReader::with_capacity(128 * 1024, File::open(f).map_err(quick_xml::Error::Io)?);
            flamegraph::from_reader(options, r, io::stdout().lock())
        }
        None => {
            let stdin = io::stdin();
            let r = BufReader::with_capacity(128 * 1024, stdin.lock());
            flamegraph::from_reader(options, r, io::stdout().lock())
        }
    }
}
