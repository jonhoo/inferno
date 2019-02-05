use std::fs::File;
use std::io::{self, BufReader};
use std::iter;
use std::path::PathBuf;
use structopt::StructOpt;

use inferno::flamegraph::{self, Options};

#[derive(Debug, StructOpt)]
#[structopt(name = "inferno-flamegraph", author = "")]
struct Opt {
    /// collapsed perf output files, or STDIN if not specified
    #[structopt(name = "INFILE", parse(from_os_str))]
    infiles: Vec<PathBuf>,
}

impl Into<Options> for Opt {
    fn into(self) -> Options {
        Options {}
    }
}

fn main() -> quick_xml::Result<()> {
    let (infiles, options) = {
        let opt = Opt::from_args();
        let infiles = opt
            .infiles
            .iter()
            .map(|f| File::open(f).map_err(quick_xml::Error::Io))
            .collect::<Result<Vec<_>, _>>()?;
        (infiles, opt.into())
    };

    if infiles.is_empty() {
        let stdin = io::stdin();
        let r = BufReader::with_capacity(128 * 1024, stdin.lock());
        flamegraph::from_readers(options, iter::once(r), io::stdout().lock())
    } else {
        flamegraph::from_readers(options, infiles, io::stdout().lock())
    }
}
