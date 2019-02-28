extern crate inferno;

use inferno::flamegraph::{self, BackgroundColor, Options, Palette};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Cursor};
use std::path::PathBuf;
use std::str::FromStr;

fn test_flamegraph(input_file: &str, expected_result_file: &str, options: Options) {
    let r = File::open(input_file).unwrap();
    let expected_len = fs::metadata(expected_result_file).unwrap().len() as usize;
    let mut result = Cursor::new(Vec::with_capacity(expected_len));
    flamegraph::from_reader(options, r, &mut result).unwrap();
    let mut expected = BufReader::new(File::open(expected_result_file).unwrap());

    result.set_position(0);

    let mut buf = String::new();
    let mut line_num = 1;
    for line in result.lines() {
        if expected.read_line(&mut buf).unwrap() == 0 {
            panic!(
                "\noutput has more lines than expected result file: {}",
                expected_result_file
            );
        }
        assert_eq!(
            line.unwrap(),
            buf.trim_end(),
            "\n{}:{}",
            expected_result_file,
            line_num
        );
        buf.clear();
        line_num += 1;
    }

    if expected.read_line(&mut buf).unwrap() > 0 {
        panic!(
            "\n{} has more lines than output, beginning at line: {}",
            expected_result_file, line_num
        )
    }
}

#[test]
fn flamegraph_colors_java() {
    let input_file = "./flamegraph/test/results/perf-java-stacks-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/colors/java.svg";

    let options = flamegraph::Options {
        colors: Palette::from_str("java").unwrap(),
        bgcolors: Some(BackgroundColor::from_str("blue").unwrap()),
        hash: true,
        title: "Flame Graph".to_string(),
        ..Default::default()
    };

    test_flamegraph(input_file, expected_result_file, options)
}

#[test]
fn flamegraph_colors_js() {
    let input_file = "./flamegraph/test/results/perf-js-stacks-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/colors/js.svg";

    let options = flamegraph::Options {
        colors: Palette::from_str("js").unwrap(),
        bgcolors: Some(BackgroundColor::from_str("green").unwrap()),
        hash: true,
        title: "Flame Graph".to_string(),
        ..Default::default()
    };

    test_flamegraph(input_file, expected_result_file, options)
}

#[test]
fn flamegraph_differential() {
    let input_file = "./tests/data/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/differential/diff.svg";
    let options = Default::default();
    test_flamegraph(input_file, expected_result_file, options);
}

#[test]
fn flamegraph_differential_negated() {
    let input_file = "./tests/data/differential/perf-cycles-instructions-01-collapsed-all-diff.txt";
    let expected_result_file = "./tests/data/differential/diff_negated.svg";
    let options = Options {
        negate_differentials: true,
        ..Default::default()
    };
    test_flamegraph(input_file, expected_result_file, options);
}

#[test]
fn flamegraph_factor() {
    let input_file = "./flamegraph/test/results/perf-vertx-stacks-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/factor/factor_2.5.svg";
    let options = Options {
        factor: 2.5,
        hash: true,
        ..Default::default()
    };
    test_flamegraph(input_file, expected_result_file, options);
}

#[test]
fn flamegraph_nameattr() {
    let input_file = "./flamegraph/test/results/perf-cycles-instructions-01-collapsed-all.txt";
    let expected_result_file = "./tests/data/nameattr/nameattr.svg";
    let nameattr_file = "./tests/data/nameattr/nameattr.txt";

    let options = flamegraph::Options {
        hash: true,
        func_frameattrs: flamegraph::FuncFrameAttrsMap::from_file(&PathBuf::from(nameattr_file))
            .unwrap(),
        title: "Flame Graph".to_string(),
        ..Default::default()
    };

    test_flamegraph(input_file, expected_result_file, options);
}
