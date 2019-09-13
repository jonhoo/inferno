mod common;

use std::fs::File;
use std::io::{self, BufReader, Cursor};
use std::process::{Command, Stdio};

use assert_cmd::prelude::*;
use inferno::collapse::vtune::{Folder, Options};
use log::Level;
use pretty_assertions::assert_eq;
use testing_logger::CapturedLog;

fn test_collapse_vtune(test_file: &str, expected_file: &str, options: Options) -> io::Result<()> {
    common::test_collapse(Folder::from(options), test_file, expected_file, false)
}

fn test_collapse_vtune_error(test_file: &str, options: Options) -> io::Error {
    common::test_collapse_error(Folder::from(options), test_file)
}

fn test_collapse_vtune_logs_with_options<F>(input_file: &str, asserter: F, options: Options)
where
    F: Fn(&Vec<CapturedLog>),
{
    common::test_collapse_logs(Folder::from(options), input_file, asserter);
}

fn test_collapse_vtune_logs<F>(input_file: &str, asserter: F)
where
    F: Fn(&Vec<CapturedLog>),
{
    test_collapse_vtune_logs_with_options(input_file, asserter, Options::default());
}

#[test]
fn collapse_vtune_default() {
    let test_file = "./tests/data/collapse-vtune/vtune.csv";
    let result_file = "./tests/data/collapse-vtune/results/vtune-default.txt";
    test_collapse_vtune(test_file, result_file, Options::default()).unwrap()
}

#[test]
fn collapse_vtune_no_modules() {
    let test_file = "./tests/data/collapse-vtune/vtune.csv";
    let result_file = "./tests/data/collapse-vtune/results/vtune-no-modules.txt";
    test_collapse_vtune(
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
fn collapse_vtune_should_log_warning_for_ending_before_header() {
    test_collapse_vtune_logs(
        "./tests/data/collapse-vtune/end-before-header.csv",
        |captured_logs| {
            let nwarnings = captured_logs
                .into_iter()
                .filter(|log| log.body == "File ended before header" && log.level == Level::Warn)
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
fn collapse_vtune_should_return_error_for_skipped_indent_level() {
    let test_file = "./tests/data/collapse-vtune/skipped-indentation.csv";
    let error = test_collapse_vtune_error(test_file, Options::default());
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error
        .to_string()
        .starts_with("Skipped indentation level at line"));
}

#[test]
fn collapse_vtune_should_return_error_for_invalid_time_field() {
    let test_file = "./tests/data/collapse-vtune/invalid-time-field.csv";
    let error = test_collapse_vtune_error(test_file, Options::default());
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error
        .to_string()
        .starts_with("Invalid `CPU Time:Self` field"));
}

#[test]
fn collapse_vtune_should_return_error_for_bad_stack_line() {
    let test_file = "./tests/data/collapse-vtune/bad-stack-line.csv";
    let error = test_collapse_vtune_error(test_file, Options::default());
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().starts_with("Unable to parse stack line"));
}

#[test]
fn collapse_vtune_cli() {
    let input_file = "./tests/data/collapse-vtune/vtune.csv";
    let expected_file = "./tests/data/collapse-vtune/results/vtune-default.txt";

    // Test with file passed in
    let output = Command::cargo_bin("inferno-collapse-vtune")
        .unwrap()
        .arg(input_file)
        .output()
        .expect("failed to execute process");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    common::compare_results(Cursor::new(output.stdout), expected, expected_file, false);

    // Test with STDIN
    let mut child = Command::cargo_bin("inferno-collapse-vtune")
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
