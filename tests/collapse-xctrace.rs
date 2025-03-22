mod common;

use std::fs::File;
use std::io::{self, BufReader, Cursor};
use std::process::{Command, Stdio};

use assert_cmd::cargo::CommandCargoExt;
use inferno::collapse::xctrace::Folder;

fn test_collapse_xctrace(test_file: &str, expected_file: &str) -> io::Result<()> {
    common::test_collapse(Folder, test_file, expected_file, false)?;
    Ok(())
}

#[test]
fn collapse_xctrace_basic() {
    let test_file = "./tests/data/collapse-xctrace/basic.xml";
    let result_file = "./tests/data/collapse-xctrace/results/basic.folded";
    test_collapse_xctrace(test_file, result_file).unwrap()
}

#[test]
fn collapse_xctrace_simple_frame_without_binary_info() {
    let test_file = "./tests/data/collapse-xctrace/simple_frame_without_binary_info.xml";
    let result_file =
        "./tests/data/collapse-xctrace/results/simple_frame_without_binary_info.folded";
    test_collapse_xctrace(test_file, result_file).unwrap()
}

#[test]
fn collapse_xctrace_cli() {
    let input_file = "./tests/data/collapse-xctrace/basic.xml";
    let expected_file = "./tests/data/collapse-xctrace/results/basic.folded";

    // Test with file passed in
    let output = Command::cargo_bin("inferno-collapse-xctrace")
        .unwrap()
        .arg(input_file)
        .output()
        .expect("failed to execute process");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    common::compare_results(Cursor::new(output.stdout), expected, expected_file, false);

    // Test with STDIN
    let mut child = Command::cargo_bin("inferno-collapse-xctrace")
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
