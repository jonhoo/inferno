use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::Path;

const READER_CAPACITY: usize = 128 * 1024;

#[derive(Default)]
struct Counts {
    first: usize,
    second: usize,
}

/// Configure the generated output.
///
/// All options default to off.
#[derive(Debug, Default)]
pub struct Options {
    /// Normalize the first profile count to match the second.
    ///
    /// This can help in scenarios where you take profiles at different times, under varying
    /// load. If you generate a differential flame graph without setting this flag, everything
    /// will look red if the load increased, or blue if it decreased. If this flag is set,
    /// the first profile is balanced so you get the full red/blue spectrum.
    pub normalize: bool,

    /// Strip hex numbers (addresses) of the form "0x45ef2173" and replace with "0x...".
    pub strip_hex: bool,
}

/// Produce an output that can be used to generate a differential flame graph.
///
/// The readers are expected to contain folded stack lines of before and after profiles with
/// the following whitespace-separated fields:
///
///  - A semicolon-separated list of frame names (e.g., `main;foo;bar;baz`).
///  - A sample count for the given stack.
///
/// The output written to the `writer` will be similar to the inputs, except there will be two
/// sample count columns -- one for each profile.
pub fn from_readers<R1, R2, W>(opt: &Options, reader1: R1, reader2: R2, writer: W) -> io::Result<()>
where
    R1: BufRead,
    R2: BufRead,
    W: Write,
{
    let mut stack_counts = HashMap::new();
    let total1 = parse_stack_counts(&opt, &mut stack_counts, reader1, true)?;
    let total2 = parse_stack_counts(&opt, &mut stack_counts, reader2, false)?;
    if opt.normalize && total1 != total2 {
        for counts in stack_counts.values_mut() {
            counts.first = counts.first * total2 / total1;
        }
    }
    write_stacks(&stack_counts, writer)
}

/// Produce an output that can be used to generate a differential flame graph from
/// a before and an after profile.
///
/// See [`from_readers`] for the input and output formats.
pub fn from_files<P1, P2, W>(
    opt: &Options,
    filename1: P1,
    filename2: P2,
    writer: W,
) -> io::Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
    W: Write,
{
    let file1 = File::open(filename1)?;
    let reader1 = io::BufReader::with_capacity(READER_CAPACITY, file1);
    let file2 = File::open(filename2)?;
    let reader2 = io::BufReader::with_capacity(READER_CAPACITY, file2);
    from_readers(opt, reader1, reader2, writer)
}

// Populate stack_counts based on lines from the reader and returns the sum of the sample counts.
fn parse_stack_counts<R>(
    opt: &Options,
    stack_counts: &mut HashMap<String, Counts>,
    mut reader: R,
    is_first: bool,
) -> io::Result<usize>
where
    R: BufRead,
{
    let mut total = 0;
    let mut line = String::new();
    loop {
        line.clear();

        if reader.read_line(&mut line)? == 0 {
            break;
        }

        if let Some((stack, count)) = parse_line(&line, opt.strip_hex) {
            let mut counts = stack_counts.entry(stack).or_default();
            if is_first {
                counts.first += count;
            } else {
                counts.second += count;
            }
            total += count;
        } else {
            warn!("Unable to parse line: {}", line);
        }
    }

    Ok(total)
}

// Write three-column lines with the folded stack trace and two value columns,
// one for each profile.
fn write_stacks<W>(stack_counts: &HashMap<String, Counts>, mut writer: W) -> io::Result<()>
where
    W: Write,
{
    for (stack, &Counts { first, second }) in stack_counts {
        writeln!(writer, "{} {} {}", stack, first, second)?;
    }
    Ok(())
}

// Parse stack and sample count from line.
fn parse_line(line: &str, strip_hex: bool) -> Option<(String, usize)> {
    let counti = line.rfind(' ')?;
    let count = &line[(counti + 1)..].trim_end();
    let count = count.parse::<usize>().ok()?;
    let mut stack = line[..counti].trim_end().to_string();
    if strip_hex {
        stack = strip_hex_address(stack);
    }
    Some((stack, count))
}

// Replace all hex strings like "0x45ef2173" with "0x...".
fn strip_hex_address(mut stack: String) -> String {
    let mut start = 0;
    while let Some(idx) = stack[start..].find("0x") {
        let ndigits = stack[start + idx + 2..]
            .chars()
            .take_while(|c| c.is_digit(16))
            .count();
        if ndigits > 0 {
            stack = format!(
                "{}0x...{}",
                &stack[..start + idx],
                &stack[start + idx + 2 + ndigits..]
            );
            start += idx + 2;
        }
    }
    stack
}
