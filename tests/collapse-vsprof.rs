mod common;

use std::fs::File;
use std::io::{self, BufReader, Cursor};
use std::process::{Command, Stdio};

use assert_cmd::prelude::*;
use inferno::collapse::vsprof::Folder;
use log::Level;
use pretty_assertions::assert_eq;
use testing_logger::CapturedLog;

fn test_collapse_vsprof(test_file: &str, expected_file: &str) -> io::Result<()> {
    common::test_collapse(Folder::default(), test_file, expected_file, false)
}

fn test_collapse_vsprof_error(test_file: &str) -> io::Error {
    common::test_collapse_error(Folder::default(), test_file)
}

fn test_collapse_vsprof_logs<F>(input_file: &str, asserter: F)
where
    F: Fn(&Vec<CapturedLog>),
{
    common::test_collapse_logs(Folder::default(), input_file, asserter);
}

#[test]
fn collapse_vsprof_default() {
    let test_file = "./tests/data/collapse-vsprof/CallTreeSummary.csv";
    let result_file = "./tests/data/collapse-vsprof/results/sample-default.txt";
    test_collapse_vsprof(test_file, result_file).unwrap()
}

#[test]
fn collapse_vsprof_should_log_warning_for_ending_before_call_graph_start() {
    test_collapse_vsprof_logs(
        "./tests/data/collapse-vsprof/empty-file.csv",
        |captured_logs| {
            let nwarnings = captured_logs
                .iter()
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
fn collapse_vsprof_should_return_error_for_incorrect_header() {
    let test_file = "./tests/data/collapse-vsprof/incorrect-header.csv";
    let error = test_collapse_vsprof_error(test_file);
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error
        .to_string()
        .starts_with("Expected first line to be header line"));
}

#[test]
fn collapse_vsprof_should_return_error_for_missing_function_name() {
    let test_file = "./tests/data/collapse-vsprof/missing-function-name.csv";
    let error = test_collapse_vsprof_error(test_file);
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error
        .to_string()
        .starts_with("Missing function name in line:"));
}

#[test]
fn collapse_vsprof_should_return_error_for_invalid_function_name() {
    let test_file = "./tests/data/collapse-vsprof/invalid-function-name.csv";
    let error = test_collapse_vsprof_error(test_file);
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error
        .to_string()
        .starts_with("Unable to parse function name from line:"));
}

#[test]
fn collapse_vsprof_should_return_error_for_invalid_depth() {
    let test_file = "./tests/data/collapse-vsprof/invalid-depth.csv";
    let error = test_collapse_vsprof_error(test_file);
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().starts_with("Unable to parse number"));
}

#[test]
fn collapse_vsprof_should_return_error_for_invalid_number_of_calls() {
    let test_file = "./tests/data/collapse-vsprof/invalid-number-of-calls.csv";
    let error = test_collapse_vsprof_error(test_file);
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error
        .to_string()
        .starts_with("Floating point numbers are not valid here"));
}

#[test]
fn collapse_vsprof_cli() {
    let input_file = "./tests/data/collapse-vsprof/CallTreeSummary.csv";
    let expected_file = "./tests/data/collapse-vsprof/results/sample-default.txt";

    // Test with file passed in
    let output = Command::cargo_bin("inferno-collapse-vsprof")
        .unwrap()
        .arg(input_file)
        .output()
        .expect("failed to execute process");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    common::compare_results(Cursor::new(output.stdout), expected, expected_file, false);

    // Test with STDIN
    let mut child = Command::cargo_bin("inferno-collapse-vsprof")
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
    common::compare_results(Cursor::new(output.stdout), expected, expected_file, false);
}
