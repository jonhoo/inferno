mod common;

use std::fs::File;
use std::io::{self, BufReader, Cursor};
use std::process::{Command, Stdio};

use assert_cmd::cargo::CommandCargoExt;
use inferno::collapse::recursive::{Folder, Options};

fn test_collapse_recursive(
    test_file: &str,
    expected_file: &str,
    options: Options,
) -> io::Result<()> {
    for &n in &[1, 2] {
        let mut options = options.clone();
        options.nthreads = n;
        common::test_collapse(Folder::from(options), test_file, expected_file, false)?;
    }
    Ok(())
}

#[test]
fn collapse_recursive_basic() {
    let test_file = "./tests/data/collapse-recursive/basic.txt";
    let result_file = "./tests/data/collapse-recursive/results/basic-collapsed.txt";
    test_collapse_recursive(test_file, result_file, Options::default()).unwrap()
}

#[test]
fn collapse_recursive_cli() {
    let input_file = "./tests/data/collapse-recursive/basic.txt";
    let expected_file = "./tests/data/collapse-recursive/results/basic-collapsed.txt";

    // Test with file passed in
    let output = Command::cargo_bin("inferno-collapse-recursive")
        .unwrap()
        .arg(input_file)
        .output()
        .expect("failed to execute process");
    let expected = BufReader::new(File::open(expected_file).unwrap());
    common::compare_results(Cursor::new(output.stdout), expected, expected_file, false);

    // Test with STDIN
    let mut child = Command::cargo_bin("inferno-collapse-recursive")
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
