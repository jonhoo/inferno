mod common;

use std::fs::File;
use std::io::{self, BufReader, Cursor};
use std::process::{Command, Stdio};

use assert_cmd::prelude::CommandCargoExt;
use inferno::collapse::ghcprof::{Folder, Options, Source};

fn test_collapse_ghcprof(test_file: &str, expected_file: &str, options: Options) -> io::Result<()> {
    common::test_collapse(Folder::from(options), test_file, expected_file, false)
}

#[test]
fn collapse_percent_default() {
    let test_file = "./tests/data/collapse-ghcprof/percent.prof";
    let result_file = "./tests/data/collapse-ghcprof/results/percent.txt";
    test_collapse_ghcprof(test_file, result_file, Options::default()).unwrap()
}

#[test]
fn collapse_ticks_default() {
    let test_file = "./tests/data/collapse-ghcprof/ticks.prof";
    let result_file = "./tests/data/collapse-ghcprof/results/ticks.txt";
    test_collapse_ghcprof(test_file, result_file, Options::default()).unwrap()
}

#[test]
fn collapse_ticks_percent() {
    let test_file = "./tests/data/collapse-ghcprof/ticks.prof";
    let result_file = "./tests/data/collapse-ghcprof/results/ticks.txt";
    let mut options = Options::default();
    options.source = Source::PercentTime;
    test_collapse_ghcprof(test_file, result_file, options).unwrap()
}

#[test]
fn collapse_ticks_ticks() {
    let test_file = "./tests/data/collapse-ghcprof/ticks.prof";
    let result_file = "./tests/data/collapse-ghcprof/results/ticks_ticks.txt";
    let mut options = Options::default();
    options.source = Source::Ticks;
    test_collapse_ghcprof(test_file, result_file, options).unwrap()
}

#[test]
fn collapse_bytes_bytes() {
    let test_file = "./tests/data/collapse-ghcprof/ticks.prof";
    let result_file = "./tests/data/collapse-ghcprof/results/ticks_bytes.txt";
    let mut options = Options::default();
    options.source = Source::Bytes;
    test_collapse_ghcprof(test_file, result_file, options).unwrap()
}

#[test]
fn collapse_utf8_default() {
    let test_file = "./tests/data/collapse-ghcprof/utf8.prof";
    let result_file = "./tests/data/collapse-ghcprof/results/utf8.txt";
    test_collapse_ghcprof(test_file, result_file, Options::default()).unwrap()
}

#[test]
fn collapse_utf8_ticks() {
    let test_file = "./tests/data/collapse-ghcprof/utf8.prof";
    let result_file = "./tests/data/collapse-ghcprof/results/utf8_ticks.txt";
    let mut options = Options::default();
    options.source = Source::Ticks;
    test_collapse_ghcprof(test_file, result_file, options).unwrap()
}

#[test]
fn collapse_utf8_bytes() {
    let test_file = "./tests/data/collapse-ghcprof/utf8.prof";
    let result_file = "./tests/data/collapse-ghcprof/results/utf8_bytes.txt";
    let mut options = Options::default();
    options.source = Source::Bytes;
    test_collapse_ghcprof(test_file, result_file, options).unwrap()
}

#[test]
fn collapse_ghcprof_cli() {
    let input_file = "./tests/data/collapse-ghcprof/ticks.prof";
    let expected_file = "./tests/data/collapse-ghcprof/results/ticks.txt";

    // Test with file passed in
    let output = Command::cargo_bin("inferno-collapse-ghcprof")
        .unwrap()
        .arg(input_file)
        .output()
        .expect("failed to execute process");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    common::compare_results(Cursor::new(output.stdout), expected, expected_file, false);

    // Test with STDIN
    let mut child = Command::cargo_bin("inferno-collapse-ghcprof")
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
