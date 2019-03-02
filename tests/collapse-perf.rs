#[macro_use]
extern crate pretty_assertions;

extern crate inferno;

use inferno::collapse::perf::{Folder, Options};
use inferno::collapse::Collapse;
use log::Level;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Cursor};
use std::path::Path;

// Create tests for test files in $dir. The test files are used as input
// and the results are compared to result files in the results sub directory.
// The test and result file names are derived from $name.
// The part after the double underscore are underscore separated options.
// For example, perf_cycles_instructions_01_pid will use the following:
//     test file: perf-cycles-instructions-01.txt
//     result file: perf-cycles-instructions-01-collapsed-pid.txt
//     flag: pid
macro_rules! collapse_perf_tests_inner {
    ($($name:ident),*; $dir:expr) => {
    $(
        #[test]
        #[allow(non_snake_case)]
        fn $name() {
            let test_name = stringify!($name);

            let mut split_name = test_name.split("__");
            let test_file_stem = split_name.next().unwrap().replace("_", "-");
            let options: Vec<_> = split_name.next().map(|s| s.split('_').collect()).unwrap_or_default();

            let test_file = format!("{}.txt", test_file_stem);
            let result_file = format!("{}-collapsed-{}.txt", test_file_stem, options.join("+"));

            let test_path = Path::new($dir);
            let results_path = test_path.join("results");

            test_collapse_perf(
                test_path.join(test_file).to_str().unwrap(),
                results_path.join(result_file).to_str().unwrap(),
                options_from_vec(options),
            ).unwrap()
        }
    )*
    }
}

macro_rules! collapse_perf_tests_upstream {
    ($($name:ident),*) => {
        collapse_perf_tests_inner!($($name),*; "./flamegraph/test");
    }
}

collapse_perf_tests_upstream! {
    perf_cycles_instructions_01__pid,
    perf_cycles_instructions_01__tid,
    perf_cycles_instructions_01__kernel,
    perf_cycles_instructions_01__jit,
    perf_cycles_instructions_01__all,
    perf_cycles_instructions_01__addrs,

    perf_dd_stacks_01__pid,
    perf_dd_stacks_01__tid,
    perf_dd_stacks_01__kernel,
    perf_dd_stacks_01__jit,
    perf_dd_stacks_01__all,
    perf_dd_stacks_01__addrs,

    perf_funcab_cmd_01__pid,
    perf_funcab_cmd_01__tid,
    perf_funcab_cmd_01__kernel,
    perf_funcab_cmd_01__jit,
    perf_funcab_cmd_01__all,
    perf_funcab_cmd_01__addrs,

    perf_funcab_pid_01__pid,
    perf_funcab_pid_01__tid,
    perf_funcab_pid_01__kernel,
    perf_funcab_pid_01__jit,
    perf_funcab_pid_01__all,
    perf_funcab_pid_01__addrs,

    perf_iperf_stacks_pidtid_01__pid,
    perf_iperf_stacks_pidtid_01__tid,
    perf_iperf_stacks_pidtid_01__kernel,
    perf_iperf_stacks_pidtid_01__jit,
    perf_iperf_stacks_pidtid_01__all,
    perf_iperf_stacks_pidtid_01__addrs,

    perf_java_faults_01__pid,
    perf_java_faults_01__tid,
    perf_java_faults_01__kernel,
    perf_java_faults_01__jit,
    perf_java_faults_01__all,
    perf_java_faults_01__addrs,

    perf_java_stacks_01__pid,
    perf_java_stacks_01__tid,
    perf_java_stacks_01__kernel,
    perf_java_stacks_01__jit,
    perf_java_stacks_01__all,
    perf_java_stacks_01__addrs,

    perf_java_stacks_02__pid,
    perf_java_stacks_02__tid,
    perf_java_stacks_02__kernel,
    perf_java_stacks_02__jit,
    perf_java_stacks_02__all,
    perf_java_stacks_02__addrs,

    perf_js_stacks_01__pid,
    perf_js_stacks_01__tid,
    perf_js_stacks_01__kernel,
    perf_js_stacks_01__jit,
    perf_js_stacks_01__all,
    perf_js_stacks_01__addrs,

    perf_mirageos_stacks_01__pid,
    perf_mirageos_stacks_01__tid,
    perf_mirageos_stacks_01__kernel,
    perf_mirageos_stacks_01__jit,
    perf_mirageos_stacks_01__all,
    perf_mirageos_stacks_01__addrs,

    perf_numa_stacks_01__pid,
    perf_numa_stacks_01__tid,
    perf_numa_stacks_01__kernel,
    perf_numa_stacks_01__jit,
    perf_numa_stacks_01__all,
    perf_numa_stacks_01__addrs,

    perf_rust_Yamakaky_dcpu__pid,
    perf_rust_Yamakaky_dcpu__tid,
    perf_rust_Yamakaky_dcpu__kernel,
    perf_rust_Yamakaky_dcpu__jit,
    perf_rust_Yamakaky_dcpu__all,
    perf_rust_Yamakaky_dcpu__addrs,

    perf_vertx_stacks_01__pid,
    perf_vertx_stacks_01__tid,
    perf_vertx_stacks_01__kernel,
    perf_vertx_stacks_01__jit,
    perf_vertx_stacks_01__all,
    perf_vertx_stacks_01__addrs
}

macro_rules! collapse_perf_tests {
    ($($name:ident),*) => {
        collapse_perf_tests_inner!($($name),*; "./tests/data/inline-counter");
    }
}

collapse_perf_tests! {
    perf_inline_counter__inline,
    perf_inline_counter__inline_context
}

fn test_collapse_perf(test_file: &str, expected_file: &str, options: Options) -> io::Result<()> {
    let r = BufReader::new(File::open(test_file)?);
    let expected_len = fs::metadata(expected_file)?.len() as usize;
    let mut result = Cursor::new(Vec::with_capacity(expected_len));
    let mut perf = Folder::from(options);
    perf.collapse(r, &mut result)?;
    let mut expected = BufReader::new(File::open(expected_file)?);

    result.set_position(0);

    let mut buf = String::new();
    let mut line_num = 1;
    for line in result.lines() {
        // Strip out " and ' since perl version does.
        let line = line?.replace("\"", "").replace("'", "");
        if expected.read_line(&mut buf)? == 0 {
            panic!(
                "\noutput has more lines than expected result file: {}",
                expected_file
            );
        }
        assert_eq!(line, buf.trim_end(), "\n{}:{}", expected_file, line_num);
        buf.clear();
        line_num += 1;
    }

    if expected.read_line(&mut buf)? > 0 {
        panic!(
            "\n{} has more lines than output, beginning at line: {}",
            expected_file, line_num
        )
    }

    Ok(())
}

fn options_from_vec(opt_vec: Vec<&str>) -> Options {
    let mut options = Options::default();
    for option in opt_vec {
        match option {
            "pid" => options.include_pid = true,
            "tid" => options.include_tid = true,
            "addrs" => options.include_addrs = true,
            "jit" => options.annotate_jit = true,
            "kernel" => options.annotate_kernel = true,
            "all" => {
                options.annotate_jit = true;
                options.annotate_kernel = true;
            }
            "inline" => options.show_inline = true,
            "context" => options.show_context = true,
            opt => panic!("invalid option: {}", opt),
        }
    }
    options
}

fn test_collapse_perf_logs<F>(input_file: &str, asserter: F)
where
    F: Fn(&Vec<testing_logger::CapturedLog>),
{
    test_collapse_perf_logs_with_options(input_file, asserter, Options::default());
}

fn test_collapse_perf_logs_with_options<F>(input_file: &str, asserter: F, options: Options)
where
    F: Fn(&Vec<testing_logger::CapturedLog>),
{
    testing_logger::setup();
    let r = BufReader::new(File::open(input_file).unwrap());
    let mut result = Cursor::new(vec![]);
    let mut perf = Folder::from(options);
    perf.collapse(r, &mut result).unwrap();
    testing_logger::validate(asserter);
}

#[test]
fn flamegraph_should_warn_about_empty_input_lines() {
    test_collapse_perf_logs("./tests/data/empty-line/empty-line.txt", |captured_logs| {
        let nwarnings = captured_logs
            .into_iter()
            .filter(|log| log.body.starts_with("weird event line: ") && log.level == Level::Warn)
            .count();
        assert_eq!(
            nwarnings, 1,
            "bad lines warning logged {} times, but should be logged exactly once",
            nwarnings
        );
    });
}

#[test]
fn flamegraph_should_warn_about_weird_input_lines() {
    test_collapse_perf_logs(
        "./tests/data/weird-stack-line/weird-stack-line.txt",
        |captured_logs| {
            let nwarnings = captured_logs
                .into_iter()
                .filter(|log| {
                    log.body.starts_with("weird stack line: ") && log.level == Level::Warn
                })
                .count();
            assert_eq!(
                nwarnings, 1,
                "bad lines warning logged {} times, but should be logged exactly once",
                nwarnings
            );
        },
    );
}
