use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Cursor};

use inferno::collapse::Collapse;
use libflate::gzip::Decoder;
use pretty_assertions::assert_eq;
use testing_logger::CapturedLog;

pub fn compare_results<R, E>(result: R, mut expected: E, expected_file: &str, strip_quotes: bool)
where
    R: BufRead,
    E: BufRead,
{
    let mut buf = String::new();
    let mut line_num = 1;
    for line in result.lines() {
        let line = if strip_quotes {
            line.unwrap().replace("\"", "").replace("'", "")
        } else {
            line.unwrap()
        };
        if expected.read_line(&mut buf).unwrap() == 0 {
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
}

pub fn test_collapse<C>(
    mut collapser: C,
    test_filename: &str,
    expected_filename: &str,
    strip_quotes: bool,
) -> io::Result<()>
where
    C: Collapse,
{
    if let Err(e) = fs::metadata(test_filename) {
        eprintln!("Failed to open input file '{}'", test_filename);
        return Err(e.into());
    }

    let mut collapse = move |out: &mut dyn io::Write| {
        if test_filename.ends_with(".gz") {
            let test_file = File::open(test_filename)?;
            let r = BufReader::new(Decoder::new(test_file).unwrap());
            collapser.collapse(r, out)
        } else {
            collapser.collapse_file(Some(test_filename), out)
        }
    };

    let metadata = match fs::metadata(expected_filename) {
        Ok(m) => m,
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                // be nice to the dev and make the file
                let mut f = File::create(expected_filename).unwrap();
                collapse(&mut f)?;
                fs::metadata(expected_filename).unwrap()
            } else {
                eprintln!("Tried to open {}.", expected_filename);
                return Err(e.into());
            }
        }
    };

    let expected_len = metadata.len() as usize;
    let mut result = Cursor::new(Vec::with_capacity(expected_len));
    let return_value = collapse(&mut result)?;
    let expected = BufReader::new(File::open(expected_filename)?);
    // write out the expected result to /tmp for easy restoration
    result.set_position(0);
    let rand: u64 = rand::random();
    let tm = std::env::temp_dir().join(format!("test-{}.folded", rand));
    if fs::write(&tm, result.get_ref()).is_ok() {
        eprintln!("test output in {}", tm.display());
    }
    // and then compare
    compare_results(result, expected, expected_filename, strip_quotes);
    Ok(return_value)
}

pub fn test_collapse_logs<C, F>(mut collapser: C, input_file: &str, asserter: F)
where
    C: Collapse,
    F: Fn(&Vec<CapturedLog>),
{
    testing_logger::setup();
    let r = BufReader::new(File::open(input_file).unwrap());
    collapser.collapse(r, std::io::sink()).unwrap();
    testing_logger::validate(asserter);
}

pub fn test_collapse_error<C>(mut collapser: C, test_filename: &str) -> io::Error
where
    C: Collapse,
{
    if fs::metadata(test_filename).is_err() {
        panic!("Failed to open input file '{}'", test_filename);
    }

    let mut collapse = move |out: &mut dyn io::Write| {
        if test_filename.ends_with(".gz") {
            let test_file = File::open(test_filename)?;
            let r = BufReader::new(Decoder::new(test_file).unwrap());
            collapser.collapse(r, out)
        } else {
            collapser.collapse_file(Some(test_filename), out)
        }
    };

    collapse(&mut io::sink()).expect_err("Expected an error")
}
