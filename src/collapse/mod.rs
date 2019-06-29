/// Stack collapsing for the output of [`dtrace`](https://www.joyent.com/dtrace).
///
/// See the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../../index.html
pub mod dtrace;

/// Attempts to use whichever Collapse implementation is appropriate for a given input
pub mod guess;

/// Stack collapsing for the output of [`perf script`](https://linux.die.net/man/1/perf-script).
///
/// See the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../../index.html
pub mod perf;

use std::cmp;
use std::fs::File;
use std::io::{self, Write};
use std::mem;
use std::path::Path;
use std::sync::Arc;

use chashmap::CHashMap;
use fnv::FnvHashMap;

const CAPACITY_INPUT_BUFFER: usize = 1024 * 1024 * 1024;
const CAPACITY_HASHMAP: usize = 512;
const READER_CAPACITY: usize = 128 * 1024;

/// The abstract behavior of stack collapsing.
///
/// Implementors of this trait are providing a way to take the stack traces produced by a
/// particular profiler's output (like `perf script`) and produce lines in the folded stack format
/// expected by [`crate::flamegraph::from_sorted_lines`].
///
/// See also the [crate-level documentation] for details.
///
///   [crate-level documentation]: ../index.html
// https://github.com/rust-lang/rust/issues/45040
// #[doc(spotlight)]
pub trait Collapse {
    /// Collapses the contents of the provided `reader` and writes folded stack lines to the
    /// provided `writer`.
    fn collapse<R, W>(&mut self, reader: R, writer: W) -> io::Result<()>
    where
        R: io::BufRead,
        W: io::Write;

    /// Collapses the contents of a file (or of STDIN if `infile` is `None`) and writes folded
    /// stack lines to provided `writer`.
    fn collapse_file<P, W>(&mut self, infile: Option<P>, writer: W) -> io::Result<()>
    where
        P: AsRef<Path>,
        W: Write,
    {
        match infile {
            Some(ref path) => {
                let file = File::open(path)?;
                let reader = io::BufReader::with_capacity(READER_CAPACITY, file);
                self.collapse(reader, writer)
            }
            None => {
                let stdio = io::stdin();
                let stdio_guard = stdio.lock();
                let reader = io::BufReader::with_capacity(READER_CAPACITY, stdio_guard);
                self.collapse(reader, writer)
            }
        }
    }

    /// Returns whether this implementation is appropriate for the given input.
    ///
    /// - `None` means "not sure -- need more input"
    /// - `Some(true)` means "yes, this implementation should work with this string"
    /// - `Some(false)` means "no, this implementation definitely won't work"
    fn is_applicable(&mut self, input: &str) -> Option<bool>;
}

/// Occurrences is a HashMap, which uses:
/// * Fnv if single-threaded
/// * CHashMap if multi-threaded
#[derive(Clone)]
enum Occurrences {
    SingleThreaded(FnvHashMap<String, usize>),
    MultiThreaded(Arc<CHashMap<String, usize>>),
}

impl Occurrences {
    fn new(nthreads: usize) -> Self {
        if nthreads <= 1 {
            let map = FnvHashMap::with_capacity_and_hasher(
                CAPACITY_HASHMAP,
                fnv::FnvBuildHasher::default(),
            );
            Occurrences::SingleThreaded(map)
        } else {
            let map = CHashMap::with_capacity(CAPACITY_HASHMAP);
            let arc = Arc::new(map);
            Occurrences::MultiThreaded(arc)
        }
    }

    fn add(&mut self, key: String, count: usize) {
        use self::Occurrences::*;
        match self {
            SingleThreaded(map) => *map.entry(key).or_insert(0) += count,
            MultiThreaded(arc) => {
                arc.upsert(key, || count, |v| *v += count);
            }
        }
    }

    fn is_concurrent(&self) -> bool {
        use self::Occurrences::*;
        match self {
            SingleThreaded(_) => false,
            MultiThreaded(_) => true,
        }
    }

    fn write<W>(&mut self, mut writer: W) -> io::Result<()>
    where
        W: io::Write,
    {
        use self::Occurrences::*;
        match self {
            SingleThreaded(ref mut map) => {
                let mut contents: Vec<_> = map.drain().collect();
                contents.sort();
                for (key, value) in contents {
                    writeln!(writer, "{} {}", key, value)?;
                }
            }
            MultiThreaded(ref mut arc) => {
                let map = match Arc::get_mut(arc) {
                    Some(map) => map,
                    None => {
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            "Attempting to drain contents of a concurrent \
                             hashmap when more than one thread has access to it, \
                             which is not allowed.",
                        ))
                    }
                };
                let map = mem::replace(map, CHashMap::with_capacity(CAPACITY_HASHMAP));
                let mut contents: Vec<_> = map.into_iter().collect();
                contents.sort();
                for (key, value) in contents {
                    writeln!(writer, "{} {}", key, value)?;
                }
            }
        }
        Ok(())
    }
}

struct Chunks<'a> {
    current_index: usize,
    data: &'a [u8],
    indices: &'a [usize],
}

impl<'a> Iterator for Chunks<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = {
            let start = self.indices.get(self.current_index)?;
            match self.indices.get(self.current_index + 1) {
                Some(end) => &self.data[*start..*end],
                None => &self.data[*start..],
            }
        };
        self.current_index += 1;
        Some(chunk)
    }
}

/// Representation of input data that can be chunked up and shared
/// across threads easily.
struct Input {
    /// Vector of byte indices representing the start of each chunk
    indices: Vec<usize>,
    /// The original data
    inner: Vec<u8>,
}

impl Input {
    fn new<F>(data: Vec<u8>, mut nthreads: usize, func: F) -> io::Result<Self>
    where
        F: FnOnce(io::BufReader<&[u8]>) -> io::Result<Vec<usize>>,
    {
        let reader = io::BufReader::new(&data[..]);

        if nthreads == 0 {
            nthreads = 1;
        }

        // Used the passed in closure to determine the
        // starting locations (byte indices) of each stack
        let stack_indices = func(reader)?;

        // If there are fewer stacks then threads, cut the number
        // of threads down to the number of stacks.
        nthreads = cmp::min(nthreads, stack_indices.len());

        // Determine starting locations (byte indices) of each chunk
        let indices = {
            let mut count = 0;
            let mut stacks_per_thread = vec![0; nthreads];
            while count < stack_indices.len() {
                let index = count % nthreads;
                stacks_per_thread[index] += 1;
                count += 1;
            }

            let mut count = 0;
            let mut indices = Vec::with_capacity(nthreads);
            for n in &stacks_per_thread {
                if *n == 0 {
                    break;
                }
                indices.push(stack_indices.get(count).cloned().unwrap());
                count += *n;
            }

            indices
        };

        Ok(Self {
            indices,
            inner: data,
        })
    }

    fn chunks(&self) -> Chunks {
        Chunks {
            current_index: 0,
            data: &self.inner[..],
            indices: &self.indices[..],
        }
    }

    fn nthreads(&mut self) -> usize {
        self.indices.len()
    }
}
