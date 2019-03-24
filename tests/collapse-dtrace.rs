#[macro_use]
extern crate pretty_assertions;

extern crate inferno;

mod collapse_common;

use assert_cmd::prelude::*;
use collapse_common::*;
use inferno::collapse::dtrace::{Folder, Options};
use log::Level;
use std::fs::File;
use std::io::{self, BufReader, Cursor};
use std::process::{Command, Stdio};

fn test_collapse_dtrace(test_file: &str, expected_file: &str, options: Options) -> io::Result<()> {
    test_collapse(Folder::from(options), test_file, expected_file)
}

fn test_collapse_dtrace_logs<F>(input_file: &str, asserter: F)
where
    F: Fn(&Vec<testing_logger::CapturedLog>),
{
    test_collapse_dtrace_logs_with_options(input_file, asserter, Options::default());
}

fn test_collapse_dtrace_logs_with_options<F>(input_file: &str, asserter: F, options: Options)
where
    F: Fn(&Vec<testing_logger::CapturedLog>),
{
    test_collapse_logs(Folder::from(options), input_file, asserter);
}

#[test]
fn collapse_dtrace_compare_to_upstream() {
    let test_file = "./flamegraph/example-dtrace-stacks.txt";
    let result_file = "./tests/data/collapse-dtrace/results/dtrace-example.txt";
    test_collapse_dtrace(test_file, result_file, Options::default()).unwrap()
}

#[test]
fn collapse_dtrace_compare_to_upstream_with_offsets() {
    let test_file = "./flamegraph/example-dtrace-stacks.txt";
    let result_file = "./tests/data/collapse-dtrace/results/dtrace-example-offsets.txt";
    test_collapse_dtrace(
        test_file,
        result_file,
        Options {
            includeoffset: true,
        },
    )
    .unwrap()
}

#[test]
fn collapse_dtrace_compare_to_upstream_java() {
    let test_file = "./tests/data/collapse-dtrace/java.txt";
    let result_file = "./tests/data/collapse-dtrace/results/java.txt";
    test_collapse_dtrace(test_file, result_file, Options::default()).unwrap()
}

#[test]
fn collapse_dtrace_hex_addresses() {
    let test_file = "./tests/data/collapse-dtrace/hex-addresses.txt";
    let result_file = "./tests/data/collapse-dtrace/results/hex-addresses.txt";
    test_collapse_dtrace(test_file, result_file, Options::default()).unwrap()
}

#[test]
fn collapse_dtrace_compare_to_flamegraph_bug() {
    // There is a bug in flamegraph that causes the following stack to render
    // badly. We fix this but keep the test around to point out this breakage
    // of bug compatibility.
    //
    // https://github.com/brendangregg/FlameGraph/issues/202
    let test_file = "./tests/data/collapse-dtrace/flamegraph-bug.txt";
    let result_file = "./tests/data/collapse-dtrace/results/flamegraph-bug.txt";
    test_collapse_dtrace(
        test_file,
        result_file,
        Options {
            includeoffset: true,
        },
    )
    .unwrap()
}

#[test]
fn collapse_dtrace_should_log_warning_for_only_header_lines() {
    test_collapse_dtrace_logs(
        "./tests/data/collapse-dtrace/only-header-lines.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body == "File ended while skipping headers" && log.level == Level::Warn
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
fn collapse_dtrace_scope_with_no_argument_list() {
    let test_file = "./tests/data/collapse-dtrace/scope_with_no_argument_list.txt";
    let result_file = "./tests/data/collapse-dtrace/results/scope_with_no_argument_list.txt";
    test_collapse_dtrace(test_file, result_file, Options::default()).unwrap()
}

#[test]
fn collapse_dtrace_cli() {
    let input_file = "./flamegraph/example-dtrace-stacks.txt";
    let expected_file = "./tests/data/collapse-dtrace/results/dtrace-example.txt";

    // Test with file passed in
    let output = Command::cargo_bin("inferno-collapse-dtrace")
        .unwrap()
        .arg(input_file)
        .output()
        .expect("failed to execute process");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    compare_results(Cursor::new(output.stdout), expected, expected_file);

    // Test with STDIN
    let mut child = Command::cargo_bin("inferno-collapse-dtrace")
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
    compare_results(Cursor::new(output.stdout), expected, expected_file);
}
