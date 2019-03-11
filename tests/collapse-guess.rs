#[macro_use]
extern crate pretty_assertions;

extern crate inferno;

mod collapse_common;

use collapse_common::*;
use inferno::collapse::guess::Folder;
use std::io;

fn test_collapse_guess(test_file: &str, expected_file: &str) -> io::Result<()> {
    test_collapse(Folder {}, test_file, expected_file)
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
