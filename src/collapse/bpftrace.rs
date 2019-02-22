//! Module containting the **bpftrace** implementation of [`Frontend`].
//!
//! [`Frontend`]: trait.Frontend.html

use std::io;
use std::iter::Peekable;
use std::mem;

use super::Frontend;

/// The bpftrace implementation of [`Frontend`].
///
/// [`Frontend`]: trait.Frontend.html
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct Bpftrace {
    state: State,
}

impl Frontend for Bpftrace {
    fn collapse<R, W>(&mut self, reader: R, mut writer: W) -> io::Result<()>
    where
        R: io::BufRead,
        W: io::Write,
    {
        self.state = State::NotInStack;

        // Buffer for when we need to write the ascii representation of a number.
        // 39 is the length, in bytes, it would take to write the largest 128 bit positive integer;
        let mut number_buffer: [u8; 39] = unsafe { mem::uninitialized() };

        // Iterate over each line
        for line in reader.lines() {
            let line = line?;

            let mut is_beginning_of_line = true;

            // Iterate over all characters in the line
            let mut chars = line.chars().peekable();
            while let Some(c) = chars.next() {
                match self.state {
                    // While we're not in a stack..
                    State::NotInStack => {
                        // If we're about to enter a stack (if we run into "@[")
                        if c == '@' && chars.peek() == Some(&'[') {
                            // Consume the '['
                            chars.next().unwrap();
                            // Transition to State::InStack
                            self.state = State::InStack(Vec::with_capacity(256));
                        } else {
                            // Otherwise, do nothing
                        }
                    }
                    // While we're in a stack...
                    State::InStack(ref mut vec) => {
                        // If we're at the end of a stack (if we run into "]:")...
                        if c == ']' && chars.peek() == Some(&':') {
                            // Consume the ':'
                            chars.next().unwrap();

                            // Consume a number
                            let number_buffer_len =
                                consume_unsigned_integer(&mut number_buffer, &mut chars)?;
                            let number = &number_buffer[..number_buffer_len];

                            // Pull out our "stack" that we've built up so far and replace it with a
                            // new, empty stack for the next round.
                            let vec = mem::replace(vec, Vec::with_capacity(256));

                            // Write our "stack" to our writer and transition to State::NotInStack
                            if !vec.is_empty() {
                                let mut first = true;
                                for s in vec.iter().rev() {
                                    if first {
                                        writer.write_all(s.as_bytes())?;
                                        first = false;
                                        continue;
                                    }
                                    writer.write_all(b";")?;
                                    writer.write_all(s.as_bytes())?;
                                }
                                writer.write_all(b" ")?;
                                writer.write_all(number)?;
                                writer.write_all(b"\n")?;

                                // Note: I think this is a bug in the original perl implementation
                                // (the next line should be outside of the if block). Leaving like
                                // this for now because it reproduces what perl code does.
                                self.state = State::NotInStack;
                            }
                        }
                        // Otherwise we're in the middle of a stack
                        else {
                            // If we're at the beginning of a new line, add an empty string to our
                            // "stack"
                            if is_beginning_of_line {
                                vec.push(String::with_capacity(256));
                            }
                            // Write the current character to last string in our "stack"
                            vec.last_mut().unwrap().push(c);
                        }
                    }
                }
                if is_beginning_of_line {
                    is_beginning_of_line = false;
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum State {
    NotInStack,
    InStack(Vec<String>),
}

impl Default for State {
    fn default() -> Self {
        State::NotInStack
    }
}

/// Consumes all whitespace, if any, until the beginning of a series of digits. Then consumes
/// the series of digits, writing their ascii representation into the provided buffer. Returns
/// the number of bytes written.
fn consume_unsigned_integer(
    buf: &mut [u8],
    chars: &mut Peekable<impl Iterator<Item = char>>,
) -> Result<usize, io::Error> {
    // We're expecting a number; so if we run out of characters, something went wrong.
    let mut next = match chars.peek() {
        Some(next) => next,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Parsing Error. Expected a number. Found '\n'.",
            ));
        }
    };

    // Consume and ignore all whitespace. Again, we're expecting a number; so if we run out
    // of characters, something went wrong.
    while next.is_whitespace() {
        chars.next().unwrap();
        next = match chars.peek() {
            Some(c) => c,
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Parsing Error. Expected a number. Found '\n'.",
                ));
            }
        }
    }

    // Consume all digits, writing their ascii representation into the provided buffer
    let mut index = 0;
    while next.is_numeric() {
        let c = chars.next().unwrap();
        buf[index] = c as u8;
        index += 1;
        next = match chars.peek() {
            Some(c) => c,
            None => &'a',
        };
    }

    // Return the number of bytes written
    Ok(index)
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::io::{self, Read};
    use std::path::Path;

    use super::*;

    #[test]
    fn test_bpftrace() {
        let root_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        let data_dir = Path::new(&root_dir)
            .join("tests")
            .join("data")
            .join("bpftrace");

        for entry in fs::read_dir(&data_dir).unwrap() {
            let path = entry.unwrap().path();
            if let Some(extension) = path.extension() {
                if extension.to_str().unwrap() == "trace" {
                    let inpath = path;
                    let outpath = inpath.with_extension("orig.folded");

                    let perl = {
                        let mut buf = Vec::new();
                        let mut reader = fs::File::open(&outpath).unwrap();
                        reader.read_to_end(&mut buf).unwrap();
                        buf
                    };

                    let rust = {
                        let mut buf = Vec::new();
                        let reader = io::BufReader::new(fs::File::open(&inpath).unwrap());
                        Bpftrace::default().collapse(reader, &mut buf).unwrap();
                        buf
                    };

                    assert_eq!(perl, rust);
                }
            }
        }
    }
}
