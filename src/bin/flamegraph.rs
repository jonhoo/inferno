use std::fs::File;
use std::io::{self, BufReader};
use structopt::StructOpt;

use inferno::flamegraph::{self, Options};

#[derive(Debug, StructOpt)]
#[structopt(name = "inferno-flamegraph", author = "")]
struct Opt {
    /// collapsed perf output file, or STDIN if not specified
    infile: Option<String>,
}

impl Into<Options> for Opt {
    fn into(self) -> Options {
        Options {}
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
