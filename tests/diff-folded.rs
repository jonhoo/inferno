mod common;

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Cursor};
use std::process::Command;

use assert_cmd::cargo::CommandCargoExt;
use inferno::differential::{self, Options};
use log::Level;
use pretty_assertions::assert_eq;
use testing_logger::CapturedLog;

fn test_diff_folded(
    infile1: &str,
    infile2: &str,
    expected_result_file: &str,
    options: Options,
) -> io::Result<()> {
    let metadata = match fs::metadata(expected_result_file) {
        Ok(m) => m,
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                // be nice to the dev and make the file
                let mut f = File::create(expected_result_file).unwrap();
                differential::from_files(options, &infile1, &infile2, &mut f)?;
                fs::metadata(expected_result_file).unwrap()
            } else {
                return Err(e.into());
            }
        }
    };

    let expected_len = metadata.len() as usize;
    let mut result = Cursor::new(Vec::with_capacity(expected_len));
    let return_value = differential::from_files(options, infile1, infile2, &mut result)?;
    let expected = BufReader::new(File::open(expected_result_file).unwrap());
    // write out the expected result to /tmp for easy restoration
    result.set_position(0);
    let rand: u64 = rand::random();
    let tm = std::env::temp_dir().join(format!("test-{}.svg", rand));
    if fs::write(&tm, result.get_ref()).is_ok() {
        eprintln!("test output in {}", tm.display());
    }
    // and then compare
    result.set_position(0);
    compare_results(result, expected, expected_result_file);
    Ok(return_value)
}

fn compare_results<R, E>(result: R, expected: E, expected_file: &str)
where
    R: BufRead,
    E: BufRead,
{
    let result_lines: Result<Vec<String>, _> = result.lines().collect();
    let mut result_lines = result_lines.unwrap();
    let expected_lines: Result<Vec<String>, _> = expected.lines().collect();
    let mut expected_lines = expected_lines.unwrap();

    assert_eq!(
        result_lines.len(),
        expected_lines.len(),
        "\nresult has {} lines, expected {} lines",
        result_lines.len(),
        expected_lines.len()
    );

    result_lines.sort_unstable();
    expected_lines.sort_unstable();

    for (line_num, (result_line, expected_line)) in result_lines
        .into_iter()
        .zip(expected_lines.into_iter())
        .enumerate()
    {
        assert_eq!(
            result_line, expected_line,
            "\n{}:{}",
            expected_file, line_num
        );
    }
}

fn test_diff_folded_logs<F>(infile1: &str, infile2: &str, asserter: F)
where
    F: Fn(&Vec<CapturedLog>),
{
    test_diff_folded_logs_with_options(infile1, infile2, asserter, Default::default());
}

fn test_diff_folded_logs_with_options<F>(
    infile1: &str,
    infile2: &str,
    asserter: F,
    options: Options,
) where
    F: Fn(&Vec<CapturedLog>),
{
    testing_logger::setup();
    let r1 = BufReader::new(File::open(infile1).unwrap());
    let r2 = BufReader::new(File::open(infile2).unwrap());
    let sink = io::sink();
    let _ = differential::from_readers(options, r1, r2, sink);
    testing_logger::validate(asserter);
}

#[test]
fn diff_folded_default() {
    let infile1 = "./tests/data/diff-folded/before.txt";
    let infile2 = "./tests/data/diff-folded/after.txt";
    let expected_result_file = "./tests/data/diff-folded/results/default.txt";

    test_diff_folded(infile1, infile2, expected_result_file, Default::default()).unwrap();
}

#[test]
fn diff_folded_normalize() {
    let infile1 = "./tests/data/diff-folded/before.txt";
    let infile2 = "./tests/data/diff-folded/after.txt";
    let expected_result_file = "./tests/data/diff-folded/results/normalize.txt";

    let opt = Options {
        normalize: true,
        ..Default::default()
    };
    test_diff_folded(infile1, infile2, expected_result_file, opt).unwrap();
}

#[test]
fn diff_folded_strip_hex() {
    let infile1 = "./tests/data/diff-folded/before.txt";
    let infile2 = "./tests/data/diff-folded/after.txt";
    let expected_result_file = "./tests/data/diff-folded/results/strip_hex.txt";

    let opt = Options {
        strip_hex: true,
        ..Default::default()
    };
    test_diff_folded(infile1, infile2, expected_result_file, opt).unwrap();
}

#[test]
fn diff_folded_fractional_samples() {
    let infile1 = "./tests/data/diff-folded/before_fractionals.txt";
    let infile2 = "./tests/data/diff-folded/after.txt";
    let expected_result_file = "./tests/data/diff-folded/results/fractionals.txt";

    test_diff_folded(infile1, infile2, expected_result_file, Default::default()).unwrap();
}

#[test]
fn diff_folded_should_log_warning_on_bad_input_line() {
    test_diff_folded_logs(
        "./tests/data/diff-folded/bad_before.txt",
        "./tests/data/diff-folded/after.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body.starts_with("Unable to parse line: ") && log.level == Level::Warn
                })
                .count();
            assert_eq!(
                nwarnings, 1,
                "bad lines warning logged {} times, but should be logged exactly once",
                nwarnings
            );
        },
    );
}

#[test]
fn diff_folded_should_log_warning_about_fractional_samples() {
    test_diff_folded_logs(
        "./tests/data/diff-folded/before_fractionals.txt",
        "./tests/data/diff-folded/after.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body == "The input data has fractional sample counts that will be truncated to integers" && log.level == Level::Warn
                })
                .count();
            assert_eq!(
                nwarnings, 1,
                "fractional samples warning logged {} times, but should be logged exactly once",
                nwarnings
            );
        },
    );
}

#[test]
fn diff_folded_cli() {
    let infile1 = "./tests/data/diff-folded/before.txt";
    let infile2 = "./tests/data/diff-folded/after.txt";
    let expected_file = "./tests/data/diff-folded/results/strip_hex.txt";

    let output = Command::cargo_bin("inferno-diff-folded")
        .unwrap()
        .arg("--strip-hex")
        .arg(infile1)
        .arg(infile2)
        .output()
        .expect("failed to execute process");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    compare_results(Cursor::new(output.stdout), expected, expected_file);
}
