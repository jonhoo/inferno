#[macro_use]
extern crate pretty_assertions;

extern crate inferno;

mod collapse_common;

use collapse_common::*;
use inferno::collapse::guess::Folder;
use log::Level;
use std::io;

fn test_collapse_guess(test_file: &str, expected_file: &str) -> io::Result<()> {
    test_collapse(Folder {}, test_file, expected_file)
}

fn test_collapse_guess_logs<F>(input_file: &str, asserter: F)
where
    F: Fn(&Vec<testing_logger::CapturedLog>),
{
    test_collapse_logs(Folder {}, input_file, asserter);
}

#[test]
fn collapse_guess_dtrace_example() {
    let test_file = "./flamegraph/example-dtrace-stacks.txt";
    let result_file = "./tests/data/collapse-dtrace/results/dtrace-example.txt";
    test_collapse_guess(test_file, result_file).unwrap()
}

#[test]
fn collapse_guess_dtrace_java() {
    let test_file = "./tests/data/collapse-dtrace/java.txt";
    let result_file = "./tests/data/collapse-dtrace/results/java.txt";
    test_collapse_guess(test_file, result_file).unwrap()
}

#[test]
fn collapse_guess_dtrace_hex_addresses() {
    let test_file = "./tests/data/collapse-dtrace/hex-addresses.txt";
    let result_file = "./tests/data/collapse-dtrace/results/hex-addresses.txt";
    test_collapse_guess(test_file, result_file).unwrap()
}

#[test]
fn collapse_guess_perf_example() {
    let test_file = "./flamegraph/example-perf-stacks.txt.gz";
    let result_file = "./tests/data/collapse-perf/results/example-perf-stacks-collapsed.txt";
    test_collapse_guess(test_file, result_file).unwrap()
}

#[test]
fn collapse_guess_perf_go_stacks() {
    let test_file = "./tests/data/collapse-perf/go-stacks.txt";
    let result_file = "./tests/data/collapse-perf/results/go-stacks-collapsed.txt";
    test_collapse_guess(test_file, result_file).unwrap()
}

#[test]
fn collapse_guess_perf_java_inline() {
    let test_file = "./tests/data/collapse-perf/java-inline.txt";
    let result_file = "./tests/data/collapse-perf/results/java-inline-collapsed.txt";
    test_collapse_guess(test_file, result_file).unwrap()
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
