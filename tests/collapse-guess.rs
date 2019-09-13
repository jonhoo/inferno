mod common;

use std::fs::File;
use std::io::{self, BufReader, Cursor};
use std::process::{Command, Stdio};

use assert_cmd::cargo::CommandCargoExt;
use inferno::collapse::guess::Folder;
use log::Level;
use pretty_assertions::assert_eq;
use testing_logger::CapturedLog;

fn test_collapse_guess(test_file: &str, expected_file: &str, strip_quotes: bool) -> io::Result<()> {
    common::test_collapse(Folder::default(), test_file, expected_file, strip_quotes)
}

fn test_collapse_guess_logs<F>(input_file: &str, asserter: F)
where
    F: Fn(&Vec<CapturedLog>),
{
    common::test_collapse_logs(Folder::default(), input_file, asserter);
}

#[test]
fn collapse_guess_dtrace_example() {
    let test_file = "./flamegraph/example-dtrace-stacks.txt";
    let result_file = "./tests/data/collapse-dtrace/results/dtrace-example.txt";
    test_collapse_guess(test_file, result_file, false).unwrap()
}

#[test]
fn collapse_guess_dtrace_java() {
    let test_file = "./tests/data/collapse-dtrace/java.txt";
    let result_file = "./tests/data/collapse-dtrace/results/java.txt";
    test_collapse_guess(test_file, result_file, false).unwrap()
}

#[test]
fn collapse_guess_dtrace_hex_addresses() {
    let test_file = "./tests/data/collapse-dtrace/hex-addresses.txt";
    let result_file = "./tests/data/collapse-dtrace/results/hex-addresses.txt";
    test_collapse_guess(test_file, result_file, false).unwrap()
}

#[test]
fn collapse_guess_perf_example() {
    let test_file = "./flamegraph/example-perf-stacks.txt.gz";
    let result_file = "./tests/data/collapse-perf/results/example-perf-stacks-collapsed.txt";
    test_collapse_guess(test_file, result_file, true).unwrap()
}

#[test]
fn collapse_guess_perf_go_stacks() {
    let test_file = "./tests/data/collapse-perf/go-stacks.txt";
    let result_file = "./tests/data/collapse-perf/results/go-stacks-collapsed.txt";
    test_collapse_guess(test_file, result_file, true).unwrap()
}

#[test]
fn collapse_guess_perf_java_inline() {
    let test_file = "./tests/data/collapse-perf/java-inline.txt";
    let result_file = "./tests/data/collapse-perf/results/java-inline-collapsed.txt";
    test_collapse_guess(test_file, result_file, true).unwrap()
}

#[test]
fn collapse_guess_sample() {
    let test_file = "./tests/data/collapse-sample/sample.txt";
    let result_file = "./tests/data/collapse-sample/results/sample-default.txt";
    test_collapse_guess(test_file, result_file, false).unwrap()
}

#[test]
fn collapse_guess_vtune() {
    let test_file = "./tests/data/collapse-vtune/vtune.csv";
    let result_file = "./tests/data/collapse-vtune/results/vtune-default.txt";
    test_collapse_guess(test_file, result_file, false).unwrap()
}

#[test]
fn collapse_guess_unknown_format_should_log_error() {
    test_collapse_guess_logs(
        "./tests/data/collapse-guess/unknown-format.txt",
        |captured_logs| {
            let nerrors = captured_logs
                .into_iter()
                .filter(|log| {
                    log.level == Level::Error
                        && log.body == "No applicable collapse implementation found for input"
                })
                .count();
            assert_eq!(
                nerrors, 1,
                "bad lines error logged {} times, but should be logged exactly once",
                nerrors
            );
        },
    );
}

#[test]
fn collapse_guess_invalid_perf_should_log_error() {
    test_collapse_guess_logs(
        "./tests/data/collapse-guess/invalid-perf-with-empty-line-after-event-line.txt",
        |captured_logs| {
            let nerrors = captured_logs
                .into_iter()
                .filter(|log| {
                    log.level == Level::Error
                        && log.body == "No applicable collapse implementation found for input"
                })
                .count();
            assert_eq!(
                nerrors, 1,
                "bad lines error logged {} times, but should be logged exactly once",
                nerrors
            );
        },
    );
}

#[test]
fn collapse_guess_cli() {
    let input_file = "./tests/data/collapse-dtrace/java.txt";
    let expected_file = "./tests/data/collapse-dtrace/results/java.txt";

    // Test with file passed in
    let output = Command::cargo_bin("inferno-collapse-guess")
        .unwrap()
        .arg(input_file)
        .output()
        .expect("failed to execute process");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    common::compare_results(Cursor::new(output.stdout), expected, expected_file, true);

    // Test with STDIN
    let mut child = Command::cargo_bin("inferno-collapse-guess")
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
