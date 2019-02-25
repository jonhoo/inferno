use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::PathBuf;
use structopt::StructOpt;

use inferno::flamegraph::{self, BackgroundColor, Direction, FuncFrameAttrsMap, Options, Palette};

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
    /// set color palette
    #[structopt(
        short = "c",
        long = "colors",
        default_value = "hot",
        raw(
            possible_values = r#"&["hot","mem","io","wakeup","java","js","perl","red","green","blue","aqua","yellow","purple","orange"]"#
        )
    )]
    colors: Palette,
    /// set background colors. Gradient choices are yellow (default), blue, green, grey; flat colors use "#rrggbb"
    #[structopt(long = "bgcolors")]
    bgcolors: Option<BackgroundColor>,
    /// colors are keyed by function name hash
    #[structopt(long = "hash")]
    hash: bool,
    /// use consistent palette (palette.map)
    #[structopt(long = "cp")]
    cp: bool,

    /// switch differential hues (green<->red)
    #[structopt(long = "negate")]
    negate: bool,
}

impl Into<Options> for Opt {
    fn into(self) -> Options {
        let mut options = Options::default();
        options.colors = self.colors;
        options.bgcolors = self.bgcolors;
        options.hash = self.hash;
        options.consistent_palette = self.cp;
        if let Some(file) = self.nameattr_file {
            match FuncFrameAttrsMap::from_file(&file) {
                Ok(m) => {
                    options.func_frameattrs = m;
                }
                Err(e) => panic!("Error reading {}: {:?}", file.display(), e),
            }
        };
        if self.inverted {
            options.direction = Direction::Inverted;
            options.title = "Icicle Graph".to_string();
        }
        options.negate_differentials = self.negate;
        options
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
