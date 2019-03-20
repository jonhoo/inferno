use env_logger::Env;
use std::io;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

use inferno::flamegraph::{
    self, color::BackgroundColor, color::PaletteMap, color::SearchColor, defaults, Direction,
    FuncFrameAttrsMap, Options, Palette,
};

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

    /// Set color palette
    #[structopt(
        short = "c",
        long = "colors",
        raw(default_value = "defaults::COLORS"),
        raw(
            possible_values = r#"&["hot","mem","io","wakeup","java","js","perl","red","green","blue","aqua","yellow","purple","orange"]"#
        )
    )]
    colors: Palette,

    /// Set background colors. Gradient choices are yellow (default), blue, green, grey; flat colors use "#rrggbb"
    #[structopt(long = "bgcolors")]
    bgcolors: Option<BackgroundColor>,

    /// Colors are keyed by function name hash
    #[structopt(long = "hash")]
    hash: bool,

    /// Use consistent palette (palette.map)
    #[structopt(long = "cp")]
    cp: bool,

    /// Search color
    #[structopt(long = "search-color", raw(default_value = "defaults::SEARCH_COLOR"))]
    search_color: SearchColor,

    /// Change title text
    #[structopt(long = "title", raw(default_value = "defaults::TITLE"))]
    title: String,

    /// Second level title (optional)
    #[structopt(long = "subtitle")]
    subtitle: Option<String>,

    /// Width of image
    #[structopt(long = "width", raw(default_value = "&defaults::str::IMAGE_WIDTH"))]
    image_width: usize,

    /// Height of each frame
    #[structopt(long = "height", raw(default_value = "&defaults::str::FRAME_HEIGHT"))]
    frame_height: usize,

    /// Omit smaller functions (default 0.1 pixels)
    #[structopt(long = "minwidth", raw(default_value = "&defaults::str::MIN_WIDTH"))]
    min_width: f64,

    /// Font type
    #[structopt(long = "fonttype", raw(default_value = "defaults::FONT_TYPE"))]
    font_type: String,

    /// Font size
    #[structopt(long = "fontsize", raw(default_value = "&defaults::str::FONT_SIZE"))]
    font_size: usize,

    /// Font width
    #[structopt(long = "fontwidth", raw(default_value = "&defaults::str::FONT_WIDTH"))]
    font_width: f64,

    /// Count type label
    #[structopt(long = "countname", raw(default_value = "defaults::COUNT_NAME"))]
    count_name: String,

    /// Name type label
    #[structopt(long = "nametype", raw(default_value = "defaults::NAME_TYPE"))]
    name_type: String,

    /// Set embedded notes in SVG
    #[structopt(long = "notes")]
    notes: Option<String>,

    /// Switch differential hues (green<->red)
    #[structopt(long = "negate")]
    negate: bool,

    /// Factor to scale sample counts by
    #[structopt(long = "factor", raw(default_value = "&defaults::str::FACTOR"))]
    factor: f64,

    /// Silence all log output
    #[structopt(short = "q", long = "quiet")]
    quiet: bool,

    /// Verbose logging mode (-v, -vv, -vvv)
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: usize,

    /// Pretty print XML with newlines and indentation.
    #[structopt(long = "pretty-xml")]
    pretty_xml: bool,

    /// Don't sort the input lines.
    /// If you set this flag you need to be sure your
    /// input stack lines are already sorted.
    #[structopt(name = "no-sort", long = "no-sort")]
    no_sort: bool,

    /// Generate stack-reversed flame graph.
    #[structopt(long = "reverse", conflicts_with = "no-sort")]
    reverse: bool,

    /// Don't include static JavaScript in flame graph.
    /// This flag is hidden since it's only meant to be used in
    /// tests so we don't have to include the same static
    /// JavaScript in all of the test files.
    #[structopt(raw(hidden = "true"), long = "no-javascript")]
    no_javascript: bool,
}

impl<'a> Opt {
    fn into_parts(self) -> (Vec<PathBuf>, Options<'a>) {
        let mut options = Options::default();
        options.colors = self.colors;
        options.bgcolors = self.bgcolors;
        options.hash = self.hash;
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
            if self.title == defaults::TITLE {
                options.title = "Icicle Graph".to_string();
            }
        }
        options.negate_differentials = self.negate;
        options.factor = self.factor;
        options.pretty_xml = self.pretty_xml;
        options.no_sort = self.no_sort;
        options.no_javascript = self.no_javascript;
        options.reverse_stack_order = self.reverse;

        // set style options
        options.subtitle = self.subtitle;
        options.image_width = self.image_width;
        options.frame_height = self.frame_height;
        options.min_width = self.min_width;
        options.font_type = self.font_type;
        options.font_size = self.font_size;
        options.font_width = self.font_width;
        options.count_name = self.count_name;
        options.name_type = self.name_type;
        if let Some(notes) = self.notes {
            options.notes = notes;
        }
        options.negate_differentials = self.negate;
        options.factor = self.factor;
        options.search_color = self.search_color;
        (self.infiles, options)
    }
}

const PALETTE_MAP_FILE: &str = "palette.map"; // default name for the palette map file

fn main() -> quick_xml::Result<()> {
    let opt = Opt::from_args();

    // Initialize logger
    if !opt.quiet {
        env_logger::Builder::from_env(Env::default().default_filter_or(match opt.verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }))
        .default_format_timestamp(false)
        .init();
    }

    let mut palette_map = match fetch_consistent_palette_if_needed(opt.cp, PALETTE_MAP_FILE) {
        Ok(palette_map) => palette_map,
        Err(e) => panic!("Error reading {}: {:?}", PALETTE_MAP_FILE, e),
    };

    let (infiles, mut options) = opt.into_parts();
    options.palette_map = palette_map.as_mut();

    flamegraph::from_files(&mut options, &infiles, io::stdout().lock())?;
    save_consistent_palette_if_needed(&palette_map, PALETTE_MAP_FILE).map_err(quick_xml::Error::Io)
}

fn fetch_consistent_palette_if_needed(
    use_consistent_palette: bool,
    palette_file: &str,
) -> io::Result<Option<PaletteMap>> {
    let palette_map = if use_consistent_palette {
        let path = Path::new(palette_file);
        Some(PaletteMap::load_from_file_or_empty(&path)?)
    } else {
        None
    };

    Ok(palette_map)
}

fn save_consistent_palette_if_needed(
    palette_map: &Option<PaletteMap>,
    palette_file: &str,
) -> io::Result<()> {
    if let Some(palette_map) = palette_map {
        let path = Path::new(palette_file);
        palette_map.save_to_file(&path)?;
    }

    Ok(())
}
