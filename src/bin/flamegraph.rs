use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::PathBuf;
use structopt::StructOpt;

use inferno::flamegraph::{self, Options, color::Palette};

#[derive(Debug, StructOpt)]
#[structopt(name = "inferno-flamegraph", author = "")]
struct Opt {
    /// Collapsed perf output files. With no INFILE, or INFILE is -, read STDIN.
    #[structopt(name = "INFILE", parse(from_os_str))]
    infiles: Vec<PathBuf>,
    /// set color palette. choices are: hot, mem, io, wakeup, java, js, perl, red, green, blue, aqua, yellow, purple, orange
    #[structopt(long = "colors", default_value = "hot")]
    colors: Palette,
    /// colors are keyed by function name hash
    #[structopt(long = "hash")]
    hash: bool,
    /// use consistent palette (palette.map)
    #[structopt(long = "cp")]
    cp: bool
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
    let opt = Opt::from_args();
    if opt.infiles.is_empty() || opt.infiles.len() == 1 && opt.infiles[0].to_str() == Some("-") {
        let stdin = io::stdin();
        let r = BufReader::with_capacity(128 * 1024, stdin.lock());
        flamegraph::from_reader(opt.into(), r, io::stdout().lock())
    } else if opt.infiles.len() == 1 {
        let r = File::open(&opt.infiles[0]).map_err(quick_xml::Error::Io)?;
        flamegraph::from_reader(opt.into(), r, io::stdout().lock())
    } else {
        let stdin = io::stdin();
        let mut stdin_added = false;
        let mut readers: Vec<Box<Read>> = Vec::with_capacity(opt.infiles.len());
        for infile in opt.infiles.iter() {
            if infile.to_str() == Some("-") {
                if !stdin_added {
                    let r = BufReader::with_capacity(128 * 1024, stdin.lock());
                    readers.push(Box::new(r));
                    stdin_added = true;
                }
            } else {
                let r = File::open(infile).map_err(quick_xml::Error::Io)?;
                readers.push(Box::new(r));
            }
        }

        flamegraph::from_readers(opt.into(), readers, io::stdout().lock())
    }
}
