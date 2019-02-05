use std::fs::File;
use std::io::{self, BufReader};
use std::path::PathBuf;
use structopt::StructOpt;

use inferno::flamegraph::{self, Options};

#[derive(Debug, StructOpt)]
#[structopt(name = "inferno-flamegraph", author = "")]
struct Opt {
    /// Collapsed perf output files. With no INFILE, or INFILE is -, read STDIN.
    #[structopt(name = "INFILE", parse(from_os_str))]
    infiles: Vec<PathBuf>,
}

impl Into<Options> for Opt {
    fn into(self) -> Options {
        Options {}
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
        let r = opt
            .infiles
            .iter()
            .map(|f| File::open(f).map_err(quick_xml::Error::Io))
            .collect::<Result<Vec<_>, _>>()?;

        flamegraph::from_readers(opt.into(), r, io::stdout().lock())
    }
}
