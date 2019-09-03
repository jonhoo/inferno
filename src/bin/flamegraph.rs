use std::io;
use std::path::{Path, PathBuf};

use env_logger::Env;
use inferno::flamegraph::color::{BackgroundColor, PaletteMap, SearchColor};
use inferno::flamegraph::{self, defaults, Direction, Options, Palette};

#[cfg(feature = "nameattr")]
use inferno::flamegraph::FuncFrameAttrsMap;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "inferno-flamegraph", about)]
struct Opt {
    // ************* //
    // *** FLAGS *** //
    // ************* //
    /// Use consistent palette (palette.map)
    #[structopt(long = "cp")]
    cp: bool,

    /// Colors are keyed by function name hash
    #[structopt(long = "hash")]
    hash: bool,

    /// Plot the flame graph up-side-down
    #[structopt(short = "i", long = "inverted")]
    inverted: bool,

    /// Switch differential hues (green<->red)
    #[structopt(long = "negate")]
    negate: bool,

    /// Don't include static JavaScript in flame graph.
    /// This flag is hidden since it's only meant to be used in
    /// tests so we don't have to include the same static
    /// JavaScript in all of the test files
    #[structopt(hidden = true, long = "no-javascript")]
    no_javascript: bool,

    /// Don't sort the input lines.
    /// If you set this flag you need to be sure your
    /// input stack lines are already sorted
    #[structopt(name = "no-sort", long = "no-sort")]
    no_sort: bool,

    /// Pretty print XML with newlines and indentation.
    #[structopt(long = "pretty-xml")]
    pretty_xml: bool,

    /// Silence all log output
    #[structopt(short = "q", long = "quiet")]
    quiet: bool,

    /// Generate stack-reversed flame graph
    #[structopt(long = "reverse", conflicts_with = "no-sort")]
    reverse: bool,

    /// Verbose logging mode (-v, -vv, -vvv)
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: usize,

    // *************** //
    // *** OPTIONS *** //
    // *************** //
    /// Set background colors. Gradient choices are yellow (default), blue, green, grey; flat colors use "#rrggbb"
    #[structopt(long = "bgcolors", value_name = "STRING")]
    bgcolors: Option<BackgroundColor>,

    /// Set color palette
    #[structopt(
        short = "c",
        long = "colors",
        default_value = defaults::COLORS,
        possible_values = &["aqua","blue","green","hot","io","java","js","mem","orange","perl","purple","red","wakeup","yellow"],
        value_name = "STRING"
    )]
    colors: Palette,

    /// Count type label
    #[structopt(
        long = "countname",
        default_value = defaults::COUNT_NAME,
        value_name = "STRING"
    )]
    countname: String,

    /// Factor to scale sample counts by
    #[structopt(
        long = "factor",
        default_value = &defaults::str::FACTOR,
        value_name = "FLOAT"
    )]
    factor: f64,

    /// Font size
    #[structopt(
        long = "fontsize",
        default_value = &defaults::str::FONT_SIZE,
        value_name = "UINT"
    )]
    fontsize: usize,

    /// Font type
    #[structopt(
        long = "fonttype",
        default_value = defaults::FONT_TYPE,
        value_name = "STRING"
    )]
    fonttype: String,

    /// Font width
    #[structopt(
        long = "fontwidth",
        default_value = &defaults::str::FONT_WIDTH,
        value_name = "FLOAT"
    )]
    fontwidth: f64,

    /// Height of each frame
    #[structopt(
        long = "height",
        default_value = &defaults::str::FRAME_HEIGHT,
        value_name = "UINT"
    )]
    height: usize,

    /// Omit functions smaller than <FLOAT> pixels
    #[structopt(
        long = "minwidth",
        default_value = &defaults::str::MIN_WIDTH,
        value_name = "FLOAT"
    )]
    minwidth: f64,

    /// File containing attributes to use for the SVG frames of particular functions.
    /// Each line in the file should be a function name followed by a tab,
    /// then a sequence of tab separated name=value pairs
    #[cfg(feature = "nameattr")]
    #[structopt(long = "nameattr", value_name = "PATH")]
    nameattr: Option<PathBuf>,

    /// Name type label
    #[structopt(
        long = "nametype",
        default_value = defaults::NAME_TYPE,
        value_name = "STRING"
    )]
    nametype: String,

    /// Set embedded notes in SVG
    #[structopt(long = "notes", value_name = "STRING")]
    notes: Option<String>,

    /// Search color
    #[structopt(
        long = "search-color",
        default_value = defaults::SEARCH_COLOR,
        value_name = "STRING"
    )]
    search_color: SearchColor,

    /// Second level title (optional)
    #[structopt(long = "subtitle", value_name = "STRING")]
    subtitle: Option<String>,

    /// Change title text
    #[structopt(
        long = "title",
        default_value = defaults::TITLE,
        value_name = "STRING"
    )]
    title: String,

    /// Width of image
    #[structopt(long = "width", value_name = "UINT")]
    width: Option<usize>,

    // ************ //
    // *** ARGS *** //
    // ************ //
    /// Collapsed perf output files. With no PATH, or PATH is -, read STDIN.
    #[structopt(name = "PATH", parse(from_os_str))]
    infiles: Vec<PathBuf>,
}

impl<'a> Opt {
    fn into_parts(self) -> (Vec<PathBuf>, Options<'a>) {
        let mut options = Options::default();
        options.title = self.title.clone();
        options.colors = self.colors;
        options.bgcolors = self.bgcolors;
        options.hash = self.hash;

        self.set_func_frameattrs(&mut options);

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
        options.image_width = self.width;
        options.frame_height = self.height;
        options.min_width = self.minwidth;
        options.font_type = self.fonttype;
        options.font_size = self.fontsize;
        options.font_width = self.fontwidth;
        options.count_name = self.countname;
        options.name_type = self.nametype;
        if let Some(notes) = self.notes {
            options.notes = notes;
        }
        options.negate_differentials = self.negate;
        options.factor = self.factor;
        options.search_color = self.search_color;
        (self.infiles, options)
    }

    #[cfg(feature = "nameattr")]
    fn set_func_frameattrs(&self, options: &mut Options) {
        if let Some(file) = &self.nameattr {
            match FuncFrameAttrsMap::from_file(&file) {
                Ok(m) => {
                    options.func_frameattrs = m;
                }
                Err(e) => panic!("Error reading {}: {:?}", file.display(), e),
            }
        };
    }

    #[cfg(not(feature = "nameattr"))]
    fn set_func_frameattrs(&self, _: &mut Options) {}
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

#[cfg(test)]
mod tests {
    use super::Opt;
    use inferno::flamegraph::{color, Direction, Options, Palette};
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;
    use std::str::FromStr;
    use structopt::StructOpt;

    #[test]
    fn default_options() {
        let args = vec!["inferno-flamegraph", "test_infile"];
        let opt = Opt::from_iter_safe(args).unwrap();
        let (_infiles, options) = opt.into_parts();
        assert_eq!(options, Options::default());
    }

    #[test]
    fn options() {
        let args = vec![
            "inferno-flamegraph",
            "--inverted",
            "--colors",
            "purple",
            "--bgcolors",
            "blue",
            "--hash",
            "--cp",
            "--search-color",
            "#203040",
            "--title",
            "Test Title",
            "--subtitle",
            "Test Subtitle",
            "--width",
            "100",
            "--height",
            "500",
            "--minwidth",
            "90.1",
            "--fonttype",
            "Helvetica",
            "--fontsize",
            "13",
            "--fontwidth",
            "10.5",
            "--countname",
            "test count name",
            "--nametype",
            "test name type",
            "--notes",
            "Test notes",
            "--negate",
            "--factor",
            "0.1",
            "--pretty-xml",
            "--reverse",
            "--no-javascript",
            "test_infile1",
            "test_infile2",
        ];
        let opt = Opt::from_iter_safe(args).unwrap();
        let (infiles, options) = opt.into_parts();
        let expected_options = Options {
            colors: Palette::from_str("purple").unwrap(),
            search_color: color::SearchColor::from_str("#203040").unwrap(),
            title: "Test Title".to_string(),
            image_width: Some(100),
            frame_height: 500,
            min_width: 90.1,
            font_type: "Helvetica".to_string(),
            font_size: 13,
            font_width: 10.5,
            count_name: "test count name".to_string(),
            name_type: "test name type".to_string(),
            factor: 0.1,
            notes: "Test notes".to_string(),
            subtitle: Some("Test Subtitle".to_string()),
            bgcolors: Some(color::BackgroundColor::Blue),
            hash: true,
            palette_map: Default::default(),
            func_frameattrs: Default::default(),
            direction: Direction::Inverted,
            negate_differentials: true,
            pretty_xml: true,
            no_sort: false,
            reverse_stack_order: true,
            no_javascript: true,
        };

        assert_eq!(options, expected_options);
        assert_eq!(infiles.len(), 2, "expected 2 input files");
        assert_eq!(infiles[0], PathBuf::from_str("test_infile1").unwrap());
        assert_eq!(infiles[1], PathBuf::from_str("test_infile2").unwrap());
    }
}
