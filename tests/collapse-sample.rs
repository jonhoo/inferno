mod collapse_common;

use assert_cmd::prelude::*;
use collapse_common::*;
use inferno::collapse::sample::{Folder, Options};
use log::Level;
use pretty_assertions::assert_eq;
use std::fs::File;
use std::io::{self, BufReader, Cursor};
use std::process::Command;

fn test_collapse_sample(test_file: &str, expected_file: &str, options: Options) -> io::Result<()> {
    test_collapse(Folder::from(options), test_file, expected_file, false)
}

fn test_collapse_sample_logs<F>(input_file: &str, asserter: F)
where
    F: Fn(&Vec<testing_logger::CapturedLog>),
{
    test_collapse_sample_logs_with_options(input_file, asserter, Options::default());
}

fn test_collapse_sample_logs_with_options<F>(input_file: &str, asserter: F, options: Options)
where
    F: Fn(&Vec<testing_logger::CapturedLog>),
{
    test_collapse_logs(Folder::from(options), input_file, asserter);
}

#[test]
fn collapse_sample_default() {
    let test_file = "./tests/data/collapse-sample/sample.txt";
    let result_file = "./tests/data/collapse-sample/results/sample-default.txt";
    test_collapse_sample(test_file, result_file, Options::default()).unwrap()
}

#[test]
fn collapse_sample_no_modules() {
    let test_file = "./tests/data/collapse-sample/sample.txt";
    let result_file = "./tests/data/collapse-sample/results/sample-no-modules.txt";
    test_collapse_sample(
        test_file,
        result_file,
        Options {
            no_modules: true,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn collapse_sample_should_log_warning_for_ending_before_call_graph_start() {
    test_collapse_sample_logs(
        "./tests/data/collapse-sample/end-before-call-graph-start.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body == "File ended before start of call graph" && log.level == Level::Warn
                })
                .count();
            assert_eq!(
                nwarnings, 1,
                "warning logged {} times, but should be logged exactly once",
                nwarnings
            );
        },
    );
}

#[test]
fn collapse_sample_should_log_warning_for_ending_before_call_graph_end() {
    test_collapse_sample_logs(
        "./tests/data/collapse-sample/end-before-call-graph-end.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body == "File ended before end of call graph" && log.level == Level::Warn
                })
                .count();
            assert_eq!(
                nwarnings, 1,
                "warning logged {} times, but should be logged exactly once",
                nwarnings
            );
        },
    );
}

#[test]
fn collapse_sample_should_log_error_for_no_four_spaces() {
    test_collapse_sample_logs(
        "./tests/data/collapse-sample/no-four-spaces.txt",
        |captured_logs| {
            let nerrors = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body
                        .starts_with("Stack line doesn't start with 4 spaces")
                        && log.level == Level::Error
                })
                .count();
            assert_eq!(
                nerrors, 1,
                "error logged {} times, but should be logged exactly once",
                nerrors
            );
        },
    );
}

#[test]
fn collapse_sample_should_log_error_for_odd_number_of_indent_chars() {
    test_collapse_sample_logs(
        "./tests/data/collapse-sample/odd-indentation.txt",
        |captured_logs| {
            let nerrors = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body
                        .starts_with("Odd number of indentation characters for line")
                        && log.level == Level::Error
                })
                .count();
            assert_eq!(
                nerrors, 1,
                "error logged {} times, but should be logged exactly once",
                nerrors
            );
        },
    );
}

#[test]
fn collapse_sample_should_log_error_for_skipped_indent_level() {
    test_collapse_sample_logs(
        "./tests/data/collapse-sample/skipped-indentation.txt",
        |captured_logs| {
            let nerrors = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body.starts_with("Skipped indentation level at line")
                        && log.level == Level::Error
                })
                .count();
            assert_eq!(
                nerrors, 1,
                "error logged {} times, but should be logged exactly once",
                nerrors
            );
        },
    );
}

#[test]
fn collapse_sample_should_log_error_for_invalid_samples_field() {
    test_collapse_sample_logs(
        "./tests/data/collapse-sample/invalid-samples-field.txt",
        |captured_logs| {
            let nerrors = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body.starts_with("Invalid samples field") && log.level == Level::Error
                })
                .count();
            assert_eq!(
                nerrors, 1,
                "error logged {} times, but should be logged exactly once",
                nerrors
            );
        },
    );
}

#[test]
fn collapse_sample_should_log_error_for_bad_stack_line() {
    test_collapse_sample_logs(
        "./tests/data/collapse-sample/bad-stack-line.txt",
        |captured_logs| {
            let nerrors = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body.starts_with("Unable to parse stack line") && log.level == Level::Error
                })
                .count();
            assert_eq!(
                nerrors, 1,
                "error logged {} times, but should be logged exactly once",
                nerrors
            );
        },
    );
}

#[test]
fn collapse_sample_should_log_error_for_stack_line_with_only_indent_chars() {
    test_collapse_sample_logs(
        "./tests/data/collapse-sample/stack-line-only-indent-chars.txt",
        |captured_logs| {
            let nerrors = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body
                        .starts_with("Found stack line with only indent characters")
                        && log.level == Level::Error
                })
                .count();
            assert_eq!(
                nerrors, 1,
                "error logged {} times, but should be logged exactly once",
                nerrors
            );
        },
    );
}

#[test]
fn collapse_sample_cli() {
    let input_file = "./tests/data/collapse-sample/sample.txt";
    let expected_file = "./tests/data/collapse-sample/results/sample-default.txt";

    // Test with file passed in
    let output = Command::cargo_bin("inferno-collapse-sample")
        .unwrap()
        .arg(input_file)
        .output()
        .expect("failed to execute process");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    compare_results(Cursor::new(output.stdout), expected, expected_file, false);

    // This is commented out because it times out on Travis CI (on Windows).
    //
    // Test with STDIN
    // let mut child = Command::cargo_bin("inferno-collapse-sample")
    //     .unwrap()
    //     .stdin(Stdio::piped())
    //     .stdout(Stdio::piped())
    //     .spawn()
    //     .expect("Failed to spawn child process");
    // let mut input = BufReader::new(File::open(input_file).unwrap());
    // let stdin = child.stdin.as_mut().expect("Failed to open stdin");
    // io::copy(&mut input, stdin).unwrap();
    // let output = child.wait_with_output().expect("Failed to read stdout");
    // let expected = BufReader::new(File::open(expected_file).unwrap());
    // compare_results(Cursor::new(output.stdout), expected, expected_file, false);
}
