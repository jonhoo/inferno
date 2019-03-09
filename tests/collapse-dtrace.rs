#[macro_use]
extern crate pretty_assertions;

extern crate inferno;

use inferno::collapse::dtrace::{Folder, Options};
use inferno::collapse::Collapse;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Cursor};

fn test_collapse_dtrace(test_file: &str, expected_file: &str, options: Options) -> io::Result<()> {
    let r = BufReader::new(
        File::open(test_file).expect(&format!("Test file {} not found.", test_file)),
    );
    let expected_len = fs::metadata(expected_file)
        .expect(&format!("Result file {} not found.", expected_file))
        .len() as usize;
    let mut result = Cursor::new(Vec::with_capacity(expected_len));
    let mut dtrace = Folder::from(options);
    let return_value = dtrace.collapse(r, &mut result);
    let mut expected = BufReader::new(
        File::open(expected_file).expect(&format!("Result file {} not found.", expected_file)),
    );

    result.set_position(0);

    let mut buf = String::new();
    let mut line_num = 1;
    for line in result.lines() {
        // Strip out " and ' since perl version does.
        let line = line.unwrap().replace("\"", "").replace("'", "");

        if expected.read_line(&mut buf).unwrap() == 0 {
            println!("extra line: {}", line);
            panic!(
                "\noutput has more lines than expected result file: {}",
                expected_file
            );
        }
        assert_eq!(line, buf.trim_end(), "\n{}:{}", expected_file, line_num);
        buf.clear();
        line_num += 1;
    }

    if expected.read_line(&mut buf).unwrap() > 0 {
        panic!(
            "\n{} has more lines than output, beginning at line: {}",
            expected_file, line_num
        )
    }

    return_value
}

#[test]
fn collapse_dtrace_compare_to_upstream() {
    let test_file = "./flamegraph/example-dtrace-stacks.txt";
    let result_file = "./tests/data/collapse-dtrace/test/results/dtrace-example.txt";
    test_collapse_dtrace(test_file, result_file, Options::default()).unwrap()
}

#[test]
fn collapse_dtrace_compare_to_upstream_with_offsets() {
    let test_file = "./flamegraph/example-dtrace-stacks.txt";
    let result_file = "./tests/data/collapse-dtrace/test/results/dtrace-example-offsets.txt";
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
    let test_file = "./tests/data/collapse-dtrace/test/java.txt";
    let result_file = "./tests/data/collapse-dtrace/test/results/java.txt";
    test_collapse_dtrace(test_file, result_file, Options::default()).unwrap()
}

#[test]
fn collapse_dtrace_compare_to_flamegraph_bug() {
    // There is a bug in flamegraph that causes the following stack to render
    // badly. We fix this but keep the test around to point out this breakage
    // of bug compatibility.
    //
    // https://github.com/brendangregg/FlameGraph/issues/202
    let test_file = "./tests/data/collapse-dtrace/test/flamegraph-bug.txt";
    let result_file = "./tests/data/collapse-dtrace/test/results/flamegraph-bug.txt";
    test_collapse_dtrace(
        test_file,
        result_file,
        Options {
            includeoffset: true,
        },
    )
    .unwrap()
}
