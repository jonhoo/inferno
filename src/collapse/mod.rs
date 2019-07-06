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

use std::fs::File;
use std::io::{self, Write};
use std::mem;
use std::path::Path;
use std::sync::Arc;

use chashmap::CHashMap;
use fnv::FnvHashMap;

const CAPACITY_HASHMAP: usize = 512;
const CAPACITY_READER: usize = 128 * 1024;

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

    /// Collapses the contents of the provided file (or of STDIN if `infile` is `None`) and
    /// writes folded stack lines to provided `writer`.
    fn collapse_file<P, W>(&mut self, infile: Option<P>, writer: W) -> io::Result<()>
    where
        P: AsRef<Path>,
        W: Write,
    {
        match infile {
            Some(ref path) => {
                let file = File::open(path)?;
                let reader = io::BufReader::with_capacity(CAPACITY_READER, file);
                self.collapse(reader, writer)
            }
            None => {
                let stdio = io::stdin();
                let stdio_guard = stdio.lock();
                let reader = io::BufReader::with_capacity(CAPACITY_READER, stdio_guard);
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
#[derive(Clone, Debug)]
pub(crate) enum Occurrences {
    SingleThreaded(FnvHashMap<String, usize>),
    MultiThreaded(Arc<CHashMap<String, usize>>),
}

impl Occurrences {
    pub(crate) fn new_single_threaded() -> Self {
        let map =
            FnvHashMap::with_capacity_and_hasher(CAPACITY_HASHMAP, fnv::FnvBuildHasher::default());
        Occurrences::SingleThreaded(map)
    }

    pub(crate) fn new_multi_threaded() -> Self {
        let map = CHashMap::with_capacity(CAPACITY_HASHMAP);
        let arc = Arc::new(map);
        Occurrences::MultiThreaded(arc)
    }

    pub(crate) fn add(&mut self, key: String, count: usize) {
        use self::Occurrences::*;
        match self {
            SingleThreaded(map) => *map.entry(key).or_insert(0) += count,
            MultiThreaded(arc) => arc.upsert(key, || count, |v| *v += count),
        }
    }

    pub(crate) fn is_concurrent(&self) -> bool {
        use self::Occurrences::*;
        match self {
            SingleThreaded(_) => false,
            MultiThreaded(_) => true,
        }
    }

    pub(crate) fn write<W>(&mut self, mut writer: W) -> io::Result<()>
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
                    None => panic!(
                        "Attempting to drain the contents of a concurrent HashMap \
                         when more than one thread has access to it, which is \
                         not allowed."
                    ),
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::{self, Read};
    use std::path::{Path, PathBuf};

    use libflate::gzip::Decoder;

    pub(crate) fn read_inputs<P>(inputs: &[P]) -> io::Result<HashMap<PathBuf, Vec<u8>>>
    where
        P: AsRef<Path>,
    {
        let mut map = HashMap::default();
        for path in inputs.iter() {
            let path = path.as_ref();
            let bytes = {
                let mut buf = Vec::new();
                let mut file = File::open(path)?;
                if path.to_str().unwrap().ends_with(".gz") {
                    let mut reader = Decoder::new(file)?;
                    reader.read_to_end(&mut buf)?;
                } else {
                    file.read_to_end(&mut buf)?;
                }
                buf
            };
            map.insert(path.to_path_buf(), bytes);
        }
        Ok(map)
    }
}
