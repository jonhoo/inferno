mod common;

use std::fs::File;
use std::io::{self, BufReader, Cursor};
use std::process::{Command, Stdio};

use assert_cmd::prelude::*;
use inferno::collapse::xdebug::{Folder, Options};

fn test_collapse_xdebug(test_file: &str, expected_file: &str, options: Options) -> io::Result<()> {
    common::test_collapse(Folder::from(options), test_file, expected_file, false)
}

#[test]
fn collapse_xdebug_default() {
    let test_file = "./tests/data/collapse-xdebug/xdebug.trace.xt";
    let result_file = "./tests/data/collapse-xdebug/results/xdebug-default.txt";
    test_collapse_xdebug(test_file, result_file, Options::default()).unwrap()
}

#[test]
fn collapse_xdebug_with_filenames() {
    let test_file = "./tests/data/collapse-xdebug/xdebug.trace.xt";
    let result_file = "./tests/data/collapse-xdebug/results/xdebug-with-filenames.txt";

    test_collapse_xdebug(
        test_file,
        result_file,
        Options {
            include_filenames: true,
            ..Options::default()
        },
    )
    .unwrap()
}

#[test]
fn collapse_xdebug_cli() {
    let input_file = "./tests/data/collapse-xdebug/xdebug.trace.xt";
    let expected_file = "./tests/data/collapse-xdebug/results/xdebug-default.txt";

    // Test with file passed in
    let output = Command::cargo_bin("inferno-collapse-xdebug")
        .unwrap()
        .arg(input_file)
        .output()
        .expect("failed to execute process");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    common::compare_results(Cursor::new(output.stdout), expected, expected_file, false);

    // Test with STDIN
    let mut child = Command::cargo_bin("inferno-collapse-xdebug")
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
