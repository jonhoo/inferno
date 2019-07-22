use std::borrow::Cow;
use std::io;
use std::mem;
use std::sync::Arc;

use chashmap::CHashMap;
use crossbeam::channel;
use fnv::FnvHashMap;
use lazy_static::lazy_static;

const CAPACITY_HASHMAP: usize = 512;
pub(crate) const CAPACITY_READER: usize = 128 * 1024;
pub(crate) const DEFAULT_NSTACKS_PER_JOB: usize = 100;
const NBYTES_PER_STACK_GUESS: usize = 1024;
const RUST_HASH_LENGTH: usize = 17;

lazy_static! {
    #[doc(hidden)]
    pub static ref DEFAULT_NTHREADS: usize = num_cpus::get();
}

/// Private trait for internal library authors.
///
/// If you implement this trait, your type will implement the public-facing
/// `Collapse` trait as well. Implementing this trait gives you parallelism
/// for free as long as you adhere to the requirements described in the
/// comments below.
pub trait CollapsePrivate: Clone + Send + Sized + Sized {
    // *********************************************************** //
    // ********************* REQUIRED METHODS ******************** //
    // *********************************************************** //

    /// Some formats, such as `dtrace`, contain a header or other non-stack
    /// information at the beginning of their input files. If header information
    /// is present, this method **must** consume it (i.e. advance the provided
    /// reader past it).
    ///
    /// This method also provides an opportunity to do processing of actual
    /// stack data on the main thread before worker threads are spun up. For
    /// an example of why this might be necessary, see `perf`, which can require
    /// reading the first stack in order to know how to process the rest.
    ///
    /// If the format you are working with does not contain header information
    /// or does not need any special, up-front processing, just have this method
    /// return `Ok(())` immediately.
    fn pre_process<R>(&mut self, reader: &mut R, occurrences: &mut Occurrences) -> io::Result<()>
    where
        R: io::BufRead;

    /// The primary method.
    ///
    /// It receives a reader whose header has already been consumed (see above), as
    /// well as a mutable reference to an `Occurences` instance (just a hashamp that
    /// works across multiple threads). Implementators should parse the stack data
    /// contained in the reader and write output to the provided `Occurrences` map.
    ///
    /// So that this method may be called multiple times, all internal
    /// state contained in `self` (e.g. any stack buffers or the like) **must** be
    /// reset before this method returns. Also, this method may **not** use threads.
    fn collapse_single_threaded<R>(
        &mut self,
        reader: R,
        occurrences: &mut Occurrences,
    ) -> io::Result<()>
    where
        R: io::BufRead;

    /// The following method is used to determine how to chunk up the input data in order to
    /// send it off to worker threads, which **must** receive full stacks (as opposed to partial
    /// stacks). Since what comprises a "stack" will vary from format to format, implementors for
    /// a specific format need to implement this method, which encodes how we know when we've
    /// reached the end of a stack.
    ///
    /// This method **must** return `true` if the provided line represents the end of a stack;
    /// `false` otherwise.
    ///
    /// If your format requires more information than merely a line of the input data in order
    /// to determine whether or not you are at the end of a stack, you can access information
    /// stored on the `self` instance, which is also available to you in this method.
    fn would_end_stack(&self, line: &[u8]) -> bool;

    /// This method should return whether the implementation corresponds with
    /// the given input string, i.e. if the input data matches the collapser.
    ///
    /// - `None` means "not sure -- need more input"
    /// - `Some(true)` means "yes, this implementation should work with this string"
    /// - `Some(false)` means "no, this implementation definitely won't work"
    fn is_applicable_(&mut self, input: &str) -> Option<bool>;

    /// This method should return the number of stacks per job to send to the threadpool.
    fn nstacks_per_job(&self) -> usize;

    /// This method should set the number of stacks per job to send to the threadpool.
    fn set_nstacks_per_job(&mut self, n: usize);

    /// This method should return the number of threads to use.
    fn nthreads(&self) -> usize;

    /// This method should set the number of threads to use.
    fn set_nthreads(&mut self, n: usize);

    // *********************************************************** //
    // ******************** PROVIDED METHODS ********************* //
    // *********************************************************** //

    fn collapse_<R, W>(&mut self, mut reader: R, writer: W) -> io::Result<()>
    where
        R: io::BufRead,
        W: io::Write,
    {
        let mut occurrences = Occurrences::new(self.nthreads());

        // Consume the header, if any, and do any other pre-processing
        // that needs to occur.
        self.pre_process(&mut reader, &mut occurrences)?;

        // Do collapsing.
        if occurrences.is_concurrent() {
            self.collapse_multi_threaded(reader, &mut occurrences)?;
        } else {
            self.collapse_single_threaded(reader, &mut occurrences)?;
        }

        // Write results.
        occurrences.write_and_clear(writer)
    }

    fn collapse_multi_threaded<R>(
        &mut self,
        mut reader: R,
        occurrences: &mut Occurrences,
    ) -> io::Result<()>
    where
        R: io::BufRead,
    {
        let nstacks_per_job = self.nstacks_per_job();
        let nthreads = self.nthreads();

        assert_ne!(nstacks_per_job, 0);
        assert!(nthreads > 1);
        assert!(occurrences.is_concurrent());

        crossbeam::thread::scope(|scope| {
            // Channel for sending an error from the worker threads to the main thread
            // in the event a worker has failed.
            let (tx_error, rx_error) = channel::bounded::<io::Error>(1);

            // Channel for sending input data from the main thread to the worker threads.
            // We choose `2 * nthreads` as the channel size here in order to limit memory
            // usage in the case of particularly large input files.
            let (tx_input, rx_input) = channel::bounded::<Option<Vec<u8>>>(2 * nthreads);

            // Channel for worker threads that have errored to signal to all the other
            // worker threads that they should stop work immediately and return.
            let (tx_stop, rx_stop) = channel::bounded::<()>(nthreads - 1);

            let mut handles = Vec::with_capacity(nthreads);
            for _ in 0..nthreads {
                let tx_error = tx_error.clone();
                let rx_input = rx_input.clone();
                let (tx_stop, rx_stop) = (tx_stop.clone(), rx_stop.clone());

                let mut folder = self.clone();
                let mut occurrences = occurrences.clone();

                // Launch the worker thread...
                let handle = scope.spawn(move |_| loop {
                    channel::select! {
                        recv(rx_input) -> input => {
                            // Received from the main thread either:
                            // * `Some(<input_data>)`, or
                            // * `None` = a signal that no more input data will be sent.
                            let data = match input.unwrap() {
                                Some(data) => data,
                                // If there is no more input data, return.
                                None => return,
                            };
                            // If there is input data, process it.
                            if let Err(e) = folder.collapse_single_threaded(&data[..], &mut occurrences) {
                                // In the event of an error...
                                //
                                // We notify all the threads about it here, rather than wait for the main input
                                // loop to see the error, so that we can also stop the input loop from iterating
                                // through the rest of the file.
                                //
                                // If the channel is full, it means another thread has also errored
                                // and already sent a stop signal to the other threads; so there is
                                // no need to wait or to check for a `SendError` here.
                                for _ in 0..(nthreads - 1) {
                                    let _ = tx_stop.try_send(());
                                }

                                // Then, send the error produced to the main thread for
                                // propagation. If the channel is full, it means another thread
                                // has also errored and already sent its error back to the
                                // main thread; so there is no need to wait or to check for a
                                // `SendError` here.
                                let _ = tx_error.try_send(e);

                                // Finally, return.
                                return;
                            }
                            // If successful, return to the top of the loop and continue to poll
                            // the input and stop channels.
                        },
                        recv(rx_stop) -> _ => {
                            // Received a signal from another worker thread that it has errored;
                            // so should cease work immediately and return.
                            return;
                        },
                    }
                });
                handles.push(handle);
            }

            // On the main thread...

            // Drop the main thread's handle to the input receiver because, while we're still
            // sending data, the way the main thread can learn that a worker thread has already
            // errored and so we should stop sending data is to get a `SendError` from input
            // channel when trying to send additional data to the worker threads, which will only
            // happen if our handle to the receiver has been dropped.
            drop(rx_input);

            let buf_capacity = usize::next_power_of_two(NBYTES_PER_STACK_GUESS * nstacks_per_job);
            let mut buf = Vec::with_capacity(buf_capacity);
            let (mut index, mut nstacks) = (0, 0);

            loop {
                let n = reader.read_until(b'\n', &mut buf)?;
                if n == 0 {
                    // If we've reached the end of the data, send the final chunk to the worker
                    // threads and break from the loop, The worker threads may or may not still
                    // be alive (depending on if one errored in between the sending of the last
                    // chunk and the sending of this one), but either way we should break the loop;
                    // so there's no need to check for a `SendError` here.
                    let _ = tx_input.send(Some(buf));
                    break;
                }
                let line = &buf[index..index + n];
                index += n;
                if self.would_end_stack(line) {
                    // If we've reached the end of a stack, count it.
                    nstacks += 1;
                    if nstacks == nstacks_per_job {
                        // If we've accumulated enough stacks to make up a chunk to send to the
                        // worker threads, try to send it.
                        let buf_capacity = usize::next_power_of_two(buf.capacity());
                        let chunk = mem::replace(&mut buf, Vec::with_capacity(buf_capacity));
                        if tx_input.send(Some(chunk)).is_err() {
                            // If sending the chunk produces a `SendError`, this means that one
                            // of the worker threads has errored, sent a signal to all the other
                            // worker threads to shut down, and they have all shutdown, in which
                            // case we know there will be an error waiting for us on the error
                            // channel; so we should stop parsing input data (i.e. break).
                            break;
                        }
                        index = 0;
                        nstacks = 0;
                    }
                    continue
                }
            }

            // We've run out of input data to send to the worker threads; so tell them.
            for _ in &handles {
                if tx_input.send(None).is_err() {
                    // If we're unable to send data on the input channel, it means one of the
                    // worker threads has errored and signaled to all the other worker threads
                    // to shutdown (and they all have), in which case, as above, we should stop
                    // trying to send information to them and proceed directly to the checking
                    // the error channel for the error that will be there.
                    break;
                }
            }

            // The main thread needs to drop it's handle to the error sender here because we
            // are about to poll the error receiver for errors, which will block until all
            // the error senders have been dropped (including ours).
            drop(tx_error);

            // Now we poll the error channel, which will block until either all work has been
            // completely successfully, in which case `maybe_error` will be `None` or an error
            // has occurred on one of the worker theads, in which case `maybe_error` will be
            // `Some(<io::Error>)`.
            let maybe_error = rx_error.iter().next();

            // All the worker threads will have exited by now; so join their handles.
            for handle in handles {
                handle.join().unwrap();
            }

            // If there was indeed an error from one of the worker threads, propagate it.
            if let Some(e) = maybe_error { Err(e)?; }

            // Otherwise, return successfully.
            Ok(())
        })
        .unwrap()
    }
}

/// Occurrences is a HashMap, which uses:
/// * Fnv if single-threaded
/// * CHashMap if multi-threaded
#[derive(Clone, Debug)]
pub enum Occurrences {
    SingleThreaded(FnvHashMap<String, usize>),
    MultiThreaded(Arc<CHashMap<String, usize>>),
}

impl Occurrences {
    pub(crate) fn new(nthreads: usize) -> Self {
        assert_ne!(nthreads, 0);
        if nthreads == 1 {
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

    /// Inserts a key-count pair into the map. If the map did not have this key
    /// present, `None` is returned. If the map did have this key present, the
    /// value is updated, and the old value is returned.
    pub(crate) fn insert(&mut self, key: String, count: usize) -> Option<usize> {
        use self::Occurrences::*;
        match self {
            SingleThreaded(map) => map.insert(key, count),
            MultiThreaded(arc) => arc.insert(key, count),
        }
    }

    /// Inserts a key-count pair into the map if the key does not already exist.
    /// If the key does already exist, adds count to the current value of the
    /// existing key.
    pub(crate) fn insert_or_add(&mut self, key: String, count: usize) {
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

    pub(crate) fn write_and_clear<W>(&mut self, mut writer: W) -> io::Result<()>
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

/// Demangles partially demangled Rust symbols that were demangled incorrectly by profilers like
/// `sample` and `DTrace`.
///
/// For example:
///     `_$LT$grep_searcher..searcher..glue..ReadByLine$LT$$u27$s$C$$u20$M$C$$u20$R$C$$u20$S$GT$$GT$::run::h30ecedc997ad7e32`
/// becomes
///     `<grep_searcher::searcher::glue::ReadByLine<'s, M, R, S>>::run`
///
/// Non-Rust symobols, or Rust symbols that are already demangled, will be returned unchanged.
///
/// Based on code in https://github.com/alexcrichton/rustc-demangle/blob/master/src/legacy.rs
#[allow(clippy::cognitive_complexity)]
pub(crate) fn fix_partially_demangled_rust_symbol(symbol: &str) -> Cow<str> {
    // Rust hashes are hex digits with an `h` prepended.
    let is_rust_hash = |s: &str| s.starts_with('h') && s[1..].chars().all(|c| c.is_digit(16));

    // If there's no trailing Rust hash just return the symbol as is.
    if symbol.len() < RUST_HASH_LENGTH || !is_rust_hash(&symbol[symbol.len() - RUST_HASH_LENGTH..])
    {
        return Cow::Borrowed(symbol);
    }

    // Strip off trailing hash.
    let mut rest = &symbol[..symbol.len() - RUST_HASH_LENGTH];

    if rest.ends_with("::") {
        rest = &rest[..rest.len() - 2];
    }

    if rest.starts_with("_$") {
        rest = &rest[1..];
    }

    let mut demangled = String::new();

    while !rest.is_empty() {
        if rest.starts_with('.') {
            if let Some('.') = rest[1..].chars().next() {
                demangled.push_str("::");
                rest = &rest[2..];
            } else {
                demangled.push_str(".");
                rest = &rest[1..];
            }
        } else if rest.starts_with('$') {
            macro_rules! demangle {
                ($($pat:expr => $demangled:expr,)*) => ({
                    $(if rest.starts_with($pat) {
                        demangled.push_str($demangled);
                        rest = &rest[$pat.len()..];
                        } else)*
                    {
                        demangled.push_str(rest);
                        break;
                    }

                })
            }

            demangle! {
                "$SP$" => "@",
                "$BP$" => "*",
                "$RF$" => "&",
                "$LT$" => "<",
                "$GT$" => ">",
                "$LP$" => "(",
                "$RP$" => ")",
                "$C$" => ",",
                "$u7e$" => "~",
                "$u20$" => " ",
                "$u27$" => "'",
                "$u3d$" => "=",
                "$u5b$" => "[",
                "$u5d$" => "]",
                "$u7b$" => "{",
                "$u7d$" => "}",
                "$u3b$" => ";",
                "$u2b$" => "+",
                "$u21$" => "!",
                "$u22$" => "\"",
            }
        } else {
            let idx = match rest.char_indices().find(|&(_, c)| c == '$' || c == '.') {
                None => rest.len(),
                Some((i, _)) => i,
            };
            demangled.push_str(&rest[..idx]);
            rest = &rest[idx..];
        }
    }

    Cow::Owned(demangled)
}

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::HashMap;
    use std::fmt;
    use std::fs::File;
    use std::io::Write;
    use std::io::{self, BufRead, Read};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use libflate::gzip::Decoder;

    use super::*;
    use crate::collapse::Collapse;

    // TODO: Eventually replace with `as_nanos`, which became part of the standard library in Rust 1.33.0.
    pub(crate) trait DurationExt {
        fn as_nanos_compat(&self) -> u128;
    }

    impl DurationExt for Duration {
        fn as_nanos_compat(&self) -> u128 {
            self.as_secs() as u128 * 1_000_000_000 + self.subsec_nanos() as u128
        }
    }

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
        C: Collapse + CollapsePrivate,
        P: AsRef<Path>,
    {
        const MAX_THREADS: usize = 16;
        for (path, bytes) in read_inputs(inputs)? {
            folder.set_nthreads(1);
            let mut writer = Vec::new();
            folder.collapse(&bytes[..], &mut writer)?;
            let expected = std::str::from_utf8(&writer[..]).unwrap();

            for n in 2..=MAX_THREADS {
                folder.set_nthreads(n);
                let mut writer = Vec::new();
                folder.collapse(&bytes[..], &mut writer)?;
                let actual = std::str::from_utf8(&writer[..]).unwrap();

                assert_eq!(
                    actual,
                    expected,
                    "Collapsing with {} threads does not produce the same output as collapsing with 1 thread for {}",
                    n,
                    path.display()
                );
            }
        }

        Ok(())
    }

    pub(crate) fn bench_nstacks<C, P>(folder: &mut C, inputs: &[P]) -> io::Result<()>
    where
        C: CollapsePrivate,
        P: AsRef<Path>,
    {
        const MIN_LINES: usize = 2000;
        const NSAMPLES: usize = 100;
        const WARMUP_SECS: usize = 3;

        let _stdout = io::stdout();
        let _stderr = io::stdout();

        let mut stdout = _stdout.lock();
        let _stderr = _stderr.lock();

        struct Foo<'a> {
            default: usize,
            nlines: usize,
            nstacks: usize,
            path: &'a Path,
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
                C: CollapsePrivate,
            {
                let default = folder.nstacks_per_job();

                let (nlines, nstacks) = count_lines_and_stacks(&bytes);
                if nlines < MIN_LINES {
                    return Ok(None);
                }

                let mut results = HashMap::default();
                let iter = vec![default]
                    .into_iter()
                    .chain(1..=10)
                    .chain((20..=nstacks).step_by(10));
                for nstacks_per_job in iter {
                    folder.set_nstacks_per_job(nstacks_per_job);
                    let mut durations = Vec::new();
                    for _ in 0..NSAMPLES {
                        let now = Instant::now();
                        folder.collapse(&bytes[..], io::sink())?;
                        durations.push(now.elapsed().as_nanos_compat());
                    }
                    let avg_duration =
                        (durations.iter().sum::<u128>() as f64 / durations.len() as f64) as u64;
                    results.insert(nstacks_per_job, avg_duration);
                    stdout.write(&[b'.'])?;
                    stdout.flush()?;
                }
                Ok(Some(Self {
                    default,
                    nlines,
                    nstacks,
                    path,
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
                let default_duration = self.results[&self.default];
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

        // Warmup
        let now = Instant::now();
        stdout.write_fmt(format_args!(
            "# Warming up for approximately {} seconds.\n",
            WARMUP_SECS
        ))?;
        stdout.flush()?;
        while now.elapsed() < std::time::Duration::from_secs(WARMUP_SECS as u64) {
            for (_, bytes) in inputs.iter() {
                folder.collapse(&bytes[..], io::sink())?;
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

#[cfg(test)]
mod tests {
    macro_rules! t {
        ($a:expr, $b:expr) => {
            assert!(ok($a, $b))
        };
    }

    macro_rules! t_unchanged {
        ($a:expr) => {
            assert!(ok_unchanged($a))
        };
    }

    fn ok(sym: &str, expected: &str) -> bool {
        let result = super::fix_partially_demangled_rust_symbol(sym);
        if result == expected {
            true
        } else {
            println!("\n{}\n!=\n{}\n", result, expected);
            false
        }
    }

    fn ok_unchanged(sym: &str) -> bool {
        let result = super::fix_partially_demangled_rust_symbol(sym);
        if result == sym {
            true
        } else {
            println!("{} should have been unchanged, but got {}", sym, result);
            false
        }
    }

    #[test]
    fn fix_partially_demangled_rust_symbols() {
        t!(
            "std::sys::unix::fs::File::open::hb90e1c1c787080f0",
            "std::sys::unix::fs::File::open"
        );
        t!("_$LT$std..fs..ReadDir$u20$as$u20$core..iter..traits..iterator..Iterator$GT$::next::hc14f1750ca79129b", "<std::fs::ReadDir as core::iter::traits::iterator::Iterator>::next");
        t!("rg::search_parallel::_$u7b$$u7b$closure$u7d$$u7d$::_$u7b$$u7b$closure$u7d$$u7d$::h6e849b55a66fcd85", "rg::search_parallel::_{{closure}}::_{{closure}}");
        t!(
            "_$LT$F$u20$as$u20$alloc..boxed..FnBox$LT$A$GT$$GT$::call_box::h8612a2a83552fc2d",
            "<F as alloc::boxed::FnBox<A>>::call_box"
        );
        t!(
            "_$LT$$RF$std..fs..File$u20$as$u20$std..io..Read$GT$::read::h5d84059cf335c8e6",
            "<&std::fs::File as std::io::Read>::read"
        );
        t!(
            "_$LT$std..thread..JoinHandle$LT$T$GT$$GT$::join::hca6aa63e512626da",
            "<std::thread::JoinHandle<T>>::join"
        );
        t!(
            "std::sync::mpsc::shared::Packet$LT$T$GT$::recv::hfde2d9e28d13fd56",
            "std::sync::mpsc::shared::Packet<T>::recv"
        );
        t!("crossbeam_utils::thread::ScopedThreadBuilder::spawn::_$u7b$$u7b$closure$u7d$$u7d$::h8fdc7d4f74c0da05", "crossbeam_utils::thread::ScopedThreadBuilder::spawn::_{{closure}}");
    }

    #[test]
    fn fix_partially_demangled_rust_symbol_on_fully_mangled_symbols() {
        t_unchanged!("_ZN4testE");
        t_unchanged!("_ZN4test1a2bcE");
        t_unchanged!("_ZN7inferno10flamegraph5merge6frames17hacfe2d67301633c2E");
        t_unchanged!("_ZN3std2rt19lang_start_internal17h540c897fe52ba9c5E");
        t_unchanged!("_ZN116_$LT$core..str..pattern..CharSearcher$LT$$u27$a$GT$$u20$as$u20$core..str..pattern..ReverseSearcher$LT$$u27$a$GT$$GT$15next_match_back17h09d544049dd719bbE");
        t_unchanged!("_ZN3std5panic12catch_unwind17h0562757d03ff60b3E");
        t_unchanged!("_ZN3std9panicking3try17h9c1cbc5599e1efbfE");
    }

    #[test]
    fn fix_partially_demangled_rust_symbol_on_fully_demangled_symbols() {
        t_unchanged!("std::sys::unix::fs::File::open");
        t_unchanged!("<F as alloc::boxed::FnBox<A>>::call_box");
        t_unchanged!("<std::fs::ReadDir as core::iter::traits::iterator::Iterator>::next");
        t_unchanged!("<rg::search::SearchWorker<W>>::search_impl");
        t_unchanged!("<grep_searcher::searcher::glue::ReadByLine<'s, M, R, S>>::run");
        t_unchanged!("<alloc::raw_vec::RawVec<T, A>>::reserve_internal");
    }
}
