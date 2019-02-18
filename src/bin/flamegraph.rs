use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::PathBuf;
use structopt::StructOpt;

use inferno::flamegraph::{self, Direction, FuncFrameAttrsMap, Options};

#[derive(Debug, StructOpt)]
#[structopt(name = "inferno-flamegraph", author = "")]
struct Opt {
    /// File containing attributes to use for the SVG frames of particular functions.
    /// Each line in the file should be a function name followed by a tab,
    /// then a sequence of tab separated name=value pairs.
    #[structopt(long = "nameattr")]
    nameattr_file: Option<PathBuf>,

    /// Plot the flame graph up-side-down.
    #[structopt(short = "i", long = "inverted")]
    inverted: bool,

    /// Collapsed perf output files. With no INFILE, or INFILE is -, read STDIN.
    #[structopt(name = "INFILE", parse(from_os_str))]
    infiles: Vec<PathBuf>,
}

impl Into<Options> for Opt {
    fn into(self) -> Options {
        let func_frameattrs = match self.nameattr_file {
            Some(file) => match FuncFrameAttrsMap::from_file(&file) {
                Ok(n) => n,
                Err(e) => panic!("Error reading {}: {:?}", file.display(), e),
            },
            None => FuncFrameAttrsMap::default(),
        };
        let direction = if self.inverted {
            Direction::Inverted
        } else {
            Direction::Straight
        };
        let title = if self.inverted {
            "Icicle Graph".to_string()
        } else {
            "Flame Graph".to_string()
        };
        Options {
            func_frameattrs,
            direction,
            title,
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
