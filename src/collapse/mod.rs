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
use lazy_static::lazy_static;

lazy_static! {
    // The following in public, but hidden, because we use it in the help text
    // of the binaries, but it doesn't need to be exposed to library users.
    #[doc(hidden)]
    pub static ref DEFAULT_NTHREADS: usize = num_cpus::get();
}

const CAPACITY_HASHMAP: usize = 512;
const CAPACITY_READER: usize = 128 * 1024;
const NBYTES_PER_STACK_GUESS: usize = 1024;
const NSTACKS_PER_JOB: usize = 100;

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

    #[cfg(test)]
    fn set_nstacks_per_job(&mut self, n: usize);

    #[cfg(test)]
    fn set_nthreads(&mut self, n: usize);
}

/// Occurrences is a HashMap, which uses:
/// * Fnv if single-threaded
/// * CHashMap if multi-threaded
#[derive(Clone, Debug)]
enum Occurrences {
    SingleThreaded(FnvHashMap<String, usize>),
    MultiThreaded(Arc<CHashMap<String, usize>>),
}

impl Occurrences {
    fn new_single_threaded() -> Self {
        let map =
            FnvHashMap::with_capacity_and_hasher(CAPACITY_HASHMAP, fnv::FnvBuildHasher::default());
        Occurrences::SingleThreaded(map)
    }

    fn new_multi_threaded() -> Self {
        let map = CHashMap::with_capacity(CAPACITY_HASHMAP);
        let arc = Arc::new(map);
        Occurrences::MultiThreaded(arc)
    }

    fn add(&mut self, key: String, count: usize) {
        use self::Occurrences::*;
        match self {
            SingleThreaded(map) => *map.entry(key).or_insert(0) += count,
            MultiThreaded(arc) => arc.upsert(key, || count, |v| *v += count),
        }
    }

    fn is_concurrent(&self) -> bool {
        use self::Occurrences::*;
        match self {
            SingleThreaded(_) => false,
            MultiThreaded(_) => true,
        }
    }

    fn write_and_clear<W>(&mut self, mut writer: W) -> io::Result<()>
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
mod tests_common {
    use std::collections::HashMap;
    use std::fmt;
    use std::fs::File;
    use std::io::{self, BufRead, Read};
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use libflate::gzip::Decoder;

    use super::*;

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

    pub(crate) fn test_collapse_multi<C, P>(folder: &mut C, inputs: &[P]) -> io::Result<()>
    where
        C: Collapse,
        P: AsRef<Path>,
    {
        const MAX_THREADS: usize = 16;
        for (_, bytes) in read_inputs(inputs)? {
            folder.set_nthreads(1);
            let mut writer = Vec::new();
            folder.collapse(&bytes[..], &mut writer)?;
            let expected = std::str::from_utf8(&writer[..]).unwrap();

            for n in 2..=MAX_THREADS {
                folder.set_nthreads(n);
                let mut writer = Vec::new();
                folder.collapse(&bytes[..], &mut writer)?;
                let actual = std::str::from_utf8(&writer[..]).unwrap();

                assert_eq!(actual, expected);
            }
        }

        Ok(())
    }

    pub(crate) fn bench_nstacks<C, P>(folder: &mut C, inputs: &[P]) -> io::Result<()>
    where
        C: Collapse,
        P: AsRef<Path>,
    {
        const MIN_LINES: usize = 2000;
        const NSAMPLES: usize = 100;
        const WARMUP_SECS: usize = 5;

        let _stdout = io::stdout();
        let _stderr = io::stdout();

        let mut stdout = _stdout.lock();
        let _stderr = _stderr.lock();

        struct Foo<'a> {
            path: &'a Path,
            nlines: usize,
            nstacks: usize,
            results: HashMap<usize, u64>,
        }

        impl<'a> Foo<'a> {
            fn new<C>(
                folder: &mut C,
                path: &'a Path,
                bytes: &[u8],
                stdout: &mut io::StdoutLock,
            ) -> io::Result<Option<Self>>
            where
                C: Collapse,
            {
                let (nlines, nstacks) = count_lines_and_stacks(&bytes);
                if nlines < MIN_LINES {
                    return Ok(None);
                }

                let mut results = HashMap::default();
                let iter = vec![1, NSTACKS_PER_JOB]
                    .into_iter()
                    .chain((10..=nstacks).step_by(10));
                for nstacks_per_job in iter {
                    folder.set_nstacks_per_job(nstacks_per_job);
                    let mut durations = Vec::new();
                    for _ in 0..NSAMPLES {
                        let mut throwaway_buffer = Vec::new();
                        let now = Instant::now();
                        folder.collapse(&bytes[..], &mut throwaway_buffer)?;
                        durations.push(now.elapsed().as_nanos());
                    }
                    let avg_duration =
                        (durations.iter().sum::<u128>() as f64 / durations.len() as f64) as u64;
                    results.insert(nstacks_per_job, avg_duration);
                    stdout.write(&[b'.'])?;
                    stdout.flush()?;
                }
                Ok(Some(Self {
                    path,
                    nlines,
                    nstacks,
                    results,
                }))
            }
        }

        impl<'a> fmt::Display for Foo<'a> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                writeln!(
                    f,
                    "{} (nstacks: {}, lines: {})",
                    self.path.display(),
                    self.nstacks,
                    self.nlines
                )?;
                let default_duration = self.results[&NSTACKS_PER_JOB];
                let mut results = self.results.iter().collect::<Vec<_>>();
                results.sort_by(|(_, d1), (_, d2)| (**d1).cmp(*d2));
                for (nstacks_per_job, duration) in results.iter().take(10) {
                    writeln!(
                        f,
                        "    nstacks_per_job: {:>4} (% of total: {:>3.0}%) | time: {:.0}% of default",
                        nstacks_per_job,
                        (**nstacks_per_job as f32 / self.nstacks as f32) * 100.0,
                        **duration as f64 / default_duration as f64 * 100.0,
                    )?;
                }
                writeln!(f)?;
                Ok(())
            }
        }

        fn count_lines_and_stacks(bytes: &[u8]) -> (usize, usize) {
            let mut reader = io::BufReader::new(bytes);
            let mut line = String::new();

            let (mut nlines, mut nstacks) = (0, 0);
            loop {
                line.clear();
                let n = reader.read_line(&mut line).unwrap();
                if n == 0 {
                    nstacks += 1;
                    break;
                }
                nlines += 1;
                if line.trim().is_empty() {
                    nstacks += 1;
                }
            }
            (nlines, nstacks)
        }

        let inputs = read_inputs(inputs)?;

        let mut throwaway_buffer = Vec::new();

        // Warmup
        let now = Instant::now();
        stdout.write_fmt(format_args!(
            "# Warming up for approximately {} seconds.\n",
            WARMUP_SECS
        ))?;
        stdout.flush()?;
        while now.elapsed() < std::time::Duration::from_secs(WARMUP_SECS as u64) {
            for (_, bytes) in inputs.iter() {
                throwaway_buffer.clear();
                folder.collapse(&bytes[..], &mut throwaway_buffer)?;
            }
        }

        // Time
        let mut foos = Vec::new();
        for (path, bytes) in &inputs {
            stdout.write_fmt(format_args!("# {} ", path.display()))?;
            stdout.flush()?;
            if let Some(foo) = Foo::new(folder, path, bytes, &mut stdout)? {
                foos.push(foo);
            }
            stdout.write(&[b'\n'])?;
            stdout.flush()?;
        }
        stdout.write(&[b'\n'])?;
        stdout.flush()?;
        foos.sort_by(|a, b| b.nstacks.cmp(&a.nstacks));
        for foo in foos {
            stdout.write_fmt(format_args!("{}", foo))?;
            stdout.flush()?;
        }

        Ok(())
    }

}
