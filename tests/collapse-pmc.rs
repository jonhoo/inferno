mod common;

use std::fs::File;
use std::io::{self, BufReader, Cursor};
use std::path::Path;
use std::process::{Command, Stdio};

use assert_cmd::cargo::CommandCargoExt;
use inferno::collapse::pmc::{Folder, Options};
use log::Level;
use pretty_assertions::assert_eq;
use testing_logger::CapturedLog;

fn test_collapse_pmc(
    test_file: &str,
    expected_file: &str,
    options: Options,
    strip_quotes: bool,
) -> io::Result<()> {
    for &n in &[1, 2] {
        let mut options = options.clone();
        options.nthreads = n;
        common::test_collapse(
            Folder::from(options),
            test_file,
            expected_file,
            strip_quotes,
        )?;
    }
    Ok(())
}

fn test_collapse_pmc_logs_with_options<F>(input_file: &str, asserter: F, mut options: Options)
where
    F: Fn(&Vec<CapturedLog>),
{
    // We must run log tests in a single thread to play nicely with `testing_logger`.
    options.nthreads = 1;
    common::test_collapse_logs(Folder::from(options), input_file, asserter);
}

fn test_collapse_pmc_logs<F>(input_file: &str, asserter: F)
where
    F: Fn(&Vec<CapturedLog>),
{
    test_collapse_pmc_logs_with_options(input_file, asserter, Options::default());
}

// Create tests for test files in $dir. The test files are used as input
// and the results are compared to result files in the results sub directory.
// The test and result file names are derived from $name.
macro_rules! collapse_pmc_tests_inner {
    ($($name:ident),*; $dir:expr; $results_dir:expr; $strip_prefix:expr, $strip_quotes:expr) => {
    $(
        #[test]
        #[allow(non_snake_case)]
        fn $name() {
            let mut test_name = stringify!($name);
            if test_name.starts_with($strip_prefix) {
                test_name = &test_name[$strip_prefix.len()..];
            }
            let test_file_stem = test_name.replace("_", "-");

            let test_file = format!("{}.txt", test_file_stem);
            let result_file = format!("{}-collapsed.txt", test_file_stem);

            let test_path = Path::new($dir);
            let results_path = Path::new($results_dir);

            test_collapse_pmc(
                test_path.join(test_file).to_str().unwrap(),
                results_path.join(result_file).to_str().unwrap(),
                Options::default(),
                $strip_quotes
            ).unwrap()
        }
    )*
    }
}

macro_rules! collapse_pmc_tests {
    ($($name:ident),*) => {
        collapse_pmc_tests_inner!($($name),*; "./tests/data/collapse-pmc"; "./tests/data/collapse-pmc/results"; "collapse_pmc_", false);
    }
}

collapse_pmc_tests! {
    collapse_pmc_simple,
    collapse_pmc_shared,
    collapse_pmc_shared2,
    collapse_pmc_dd,
    collapse_pmc_iperf3
}

#[test]
fn collapse_pmc_should_warn_about_weird_input_lines_bad_percent() {
    test_collapse_pmc_logs(
        "./tests/data/collapse-pmc/weird-stack-line-bad-percent.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .iter()
                .filter(|log| {
                    log.body.starts_with("Weird stack line: ") && log.level == Level::Warn
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
fn collapse_pmc_should_warn_about_weird_input_lines_bad_count() {
    // bad count
    test_collapse_pmc_logs(
        "./tests/data/collapse-pmc/weird-stack-line-bad-count.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .iter()
                .filter(|log| {
                    log.body.starts_with("Weird stack line: ") && log.level == Level::Warn
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
fn collapse_pmc_should_warn_about_weird_input_lines_no_function_name() {
    // no funcname
    test_collapse_pmc_logs(
        "./tests/data/collapse-pmc/weird-stack-line-no-function-name.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .iter()
                .filter(|log| {
                    log.body.starts_with("Weird stack line: ") && log.level == Level::Warn
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
fn collapse_pmc_cli() {
    let input_file = "./tests/data/collapse-pmc/dd.txt";
    let expected_file = "./tests/data/collapse-pmc/results/dd-collapsed.txt";

    // Test with file passed in
    let output = Command::cargo_bin("inferno-collapse-pmc")
        .unwrap()
        .arg(input_file)
        .output()
        .expect("failed to execute process");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    common::compare_results(Cursor::new(output.stdout), expected, expected_file, true);

    // Test with STDIN
    let mut child = Command::cargo_bin("inferno-collapse-pmc")
        .unwrap()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn child process");
    let mut input = BufReader::new(File::open(input_file).unwrap());
    let stdin = child.stdin.as_mut().expect("Failed to open stdin");
    io::copy(&mut input, stdin).unwrap();
    let output = child.wait_with_output().expect("Failed to read stdout");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    common::compare_results(Cursor::new(output.stdout), expected, expected_file, true);
}
