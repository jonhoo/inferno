use super::Collapse;
use crate::collapse::common;
use crate::collapse::common::Occurrences;
use log::error;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::io::prelude::*;
use std::io::{self, Write};

const MAIN: &str = "{main}";
const TRACE_START: &str = "TRACE START";
const TRACE_SUMMARY: &str = "summary: ";

/// Options for the Xdebug collapser
#[derive(Clone, Debug)]
pub struct Options {
    /// The number of threads to use.
    ///
    /// Default is the number of logical cores on your machine.
    pub nthreads: usize,

    /// Whether to write the filenames that called functions are contained
    /// in in the output.
    pub include_filenames: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            nthreads: *common::DEFAULT_NTHREADS,
            include_filenames: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Call {
    WithPath(usize, usize),
    WithoutPath(usize),
}

#[derive(Clone, Debug)]
struct Function {
    function: Call,
    calls: Vec<Function>,
}

/// The Folder struct
#[derive(Debug, Default)]
pub struct Folder {
    filenames: HashMap<usize, Option<String>>,
    function_names: HashMap<usize, String>,
    /// Functions that have been seen so far, but not yet called. A single function may
    /// appear multiple times, with multiple callers. The file format guarantees that
    /// the function is "defined" before it appears as a called function (cfn).
    function_cache: HashMap<usize, Function>,
    options: Options,
}

impl From<Options> for Folder {
    fn from(options: Options) -> Self {
        Self {
            options,
            ..Default::default()
        }
    }
}

impl Collapse for Folder {
    fn collapse<R, W>(&mut self, mut reader: R, writer: W) -> io::Result<()>
    where
        R: BufRead,
        W: Write,
    {
        // Read buffer
        let mut line = String::new();
        let mut occurrences = Occurrences::new(1);

        // TODO: Handle header correctly.
        loop {
            reader.read_line(&mut line)?;
            if line.trim().is_empty() {
                break;
            }

            line.clear();
        }
        loop {
            reader.read_line(&mut line)?;
            if line.trim().is_empty() {
                break;
            }

            line.clear();
        }

        loop {
            line.clear();
            reader.read_line(&mut line)?;

            if line.trim().is_empty() {
                break;
            }

            // Stop if we reached the end.
            if line.starts_with(TRACE_SUMMARY) {
                break;
            }

            // Process each function, where the format is as follows:
            //
            //   fl=(n) filename
            // Where n is a unique number for the file containing the function, and the filename may
            // be omitted for PHP's built-in functions, in which case n=1, or when the filename had
            // a previous occurrence.
            //
            //   fn=(n) function:filename
            // Again, where n is a number uniquely identifying the function, and the filename is
            // omitted for PHP's built-in functions, e.g.:
            //   fn=(n) function
            //
            // This is followed by a stats line:
            //   x y z
            // Where x, y, and z are all unsigned integer, signifying the line number, time, and
            // memory, respectively.
            //
            // This is then followed by all (0 or more) functions called by this function, all in
            // the format of:
            //
            //   cfl=(a)
            //   cfn=(b)
            //   calls=1 0 0
            //   c d e
            //
            // Where:
            // - a is the unique number of the filename containing the called function,
            // - b is the unique number of the called function,
            // - and c, d, and e are again the line number, time, and memory.
            //
            // I don't know what `calls=1 0 0` is supposed to signify. Reference:
            // https://github.com/xdebug/xdebug/blob/393c8f6aed0fc1e63516b7f7f75da06480d82df3/src/profiler/profiler.c#L392
            //
            // So a full example would be:
            //
            // fl=(7) /vendor/symfony/polyfill-php80/bootstrap.php
            // fn=(22) require::/vendor/symfony/polyfill-php80/bootstrap.php
            // 1 1940 0
            // cfl=(1)
            // cfn=(2)
            // calls=1 0 0
            // 19 96 24
            // cfl=(1)
            // cfn=(21)
            // calls=1 0 0
            // 40 22 0

            // Note that we've already read the line starting with fl= by this point.
            let (file_index, filename) = self.get_index_and_optional_name(&line, "fl", None);
            let (function_index, function) = self.read_index_and_optional_name(
                &mut reader,
                &mut line,
                "fn",
                filename.as_deref(),
            )?;

            let call = match filename {
                None => Call::WithoutPath(function_index),
                Some(_) => Call::WithPath(function_index, file_index),
            };

            let is_main = function
                .as_ref()
                .map(|name| name == MAIN)
                .unwrap_or_default();

            self.filenames.entry(file_index).or_insert(filename);
            self.function_names
                .entry(function_index)
                .or_insert_with(|| function.expect("function name is not optional"));

            let (_line_number, _time, _memory) = self.read_call_stats(&mut reader, &mut line)?;
            let mut current_function = Function::new(call);

            // Now read all calls from this function
            loop {
                line.clear();
                reader.read_line(&mut line)?;

                if line.trim().is_empty() {
                    // Done with this function.
                    break;
                }

                let (_called_file_id, _) = self.get_index_and_optional_name(&line, "cfl", None);
                let (called_function_id, _) =
                    self.read_index_and_optional_name(&mut reader, &mut line, "cfn", None)?;

                // Skip line "calls=1 0 0"
                reader.read_line(&mut line)?;

                let (_, _call_time, _) = self.read_call_stats(&mut reader, &mut line)?;

                match self.function_cache.get(&called_function_id) {
                    Some(f) => current_function.call(f.clone()), // TODO: Can we remove this clone()?
                    None => error!("undefined called function {}", called_function_id),
                }
            }

            if is_main {
                current_function.gather_stacks(self, &mut occurrences);
                break;
            } else {
                self.function_cache
                    .entry(function_index)
                    // TODO: What does it mean if the function is defined multiple times, without
                    // being called in between those definitions? Should we keep a queue of them?
                    .or_insert(current_function);
            }
        }

        occurrences.write_and_clear(writer)
    }

    fn is_applicable(&mut self, input: &str) -> Option<bool> {
        let mut input = input.as_bytes();
        let mut line = String::new();
        loop {
            if let Ok(n) = input.read_line(&mut line) {
                if n == 0 {
                    break;
                } else {
                    return Some(false);
                }
            }

            if line.starts_with(TRACE_START) {
                return Some(true);
            }
        }

        None
    }
}

impl Folder {
    /// Read a line from the [reader] and get the identifier and its optional
    /// name using [Self::get_index_and_optional_name].
    fn read_index_and_optional_name<R>(
        &self,
        reader: &mut R,
        line: &mut String,
        expected_prefix: &str,
        strip_suffix: Option<&str>,
    ) -> io::Result<(usize, Option<String>)>
    where
        R: BufRead,
    {
        line.clear();

        reader.read_line(line)?;
        Ok(self.get_index_and_optional_name(line, expected_prefix, strip_suffix))
    }

    /// Given a line from the Xdebug trace file, extract the numerical identifier,
    /// and an optional string value. The format of the line is:
    /// {expected_prefix}=({identifier}) {filename}
    ///
    /// The filename part is optional, the alternative format is:
    /// {expected_prefix}=({identifier}) {filename}
    ///
    /// If a [strip_suffix] is given, it is stripped from the filename, if any.
    fn get_index_and_optional_name(
        &self,
        line: &str,
        expected_prefix: &str,
        strip_suffix: Option<&str>,
    ) -> (usize, Option<String>) {
        if !line.starts_with(expected_prefix) {
            error!("Invalid line {}, expected prefix {}", line, expected_prefix);
            return (0, None);
        }

        let line = line.trim();
        if let Some((a, b)) = line.split_once(' ') {
            (
                self.get_prefixed_id(a, expected_prefix),
                Some(match strip_suffix {
                    Some(suffix) => b
                        .strip_suffix(&format!(":{}", suffix))
                        .unwrap_or(b)
                        .to_owned(),
                    None => b.to_owned(),
                }),
            )
        } else {
            (self.get_prefixed_id(line, expected_prefix), None)
        }
    }

    /// Get a reference to an id, stripping off the prefix and other characters. The format is:
    ///
    /// {prefix}=({id})
    fn get_prefixed_id(&self, str: &str, prefix: &str) -> usize {
        str[prefix.len() + 2..str.len() - 1]
            .parse()
            .unwrap_or_else(|_| panic!("unable to parse {} index", prefix))
    }

    /// Read a line with stats about a function or function call. Such as line has three unsigned
    /// integers, separated by spaces, signifying the line number, time spent, and memory used.
    fn read_call_stats<R>(
        &self,
        reader: &mut R,
        line: &mut String,
    ) -> io::Result<(usize, f64, usize)>
    where
        R: BufRead,
    {
        line.clear();
        reader.read_line(line)?;

        let mut parts = line.trim().split(' ');

        // TODO: Solve these unwrap calls
        let line_number = parts.next().unwrap().parse().unwrap();
        let time_ns = parts.next().unwrap().parse().unwrap();
        let mem = parts.next().unwrap().parse().unwrap();

        Ok((line_number, time_ns, mem))
    }
}

#[allow(clippy::unused_io_amount)]
impl Function {
    /// Create a function instance, that can keep track of the other functions it calls.
    pub fn new(function: Call) -> Self {
        Function {
            function,
            calls: Vec::with_capacity(16),
        }
    }

    /// Does this function call no other functions?
    fn is_tail(&self) -> bool {
        self.calls.is_empty()
    }

    /// Push a `call` line that is called by this function.
    pub fn call(&mut self, function: Function) {
        self.calls.push(function);
    }

    /// Gather all stacks, uses [Self::gather_stacks_recursive].
    fn gather_stacks(&self, folder: &Folder, occurrences: &mut Occurrences) {
        let mut seen = HashSet::with_capacity(16);
        self.gather_stacks_recursive(
            &mut String::with_capacity(1024),
            &mut seen,
            folder,
            occurrences,
        );
    }

    /// Recursively walk all called functions, until each tail is reached, at which point it's added
    /// to the occurrences list.
    fn gather_stacks_recursive(
        &self,
        key: &mut String,
        seen: &mut HashSet<usize>,
        folder: &Folder,
        occurrences: &mut Occurrences,
    ) {
        let old_prefix_len = key.len();
        if !key.is_empty() {
            key.push(';');
        }
        key.push_str(&self.function.as_str(folder));

        if self.is_tail() {
            occurrences.insert_or_add(key.clone(), 1);
            key.truncate(old_prefix_len);
            return;
        }

        for call in &self.calls {
            let func_id = call.function.get_function_id();
            if seen.contains(&func_id) {
                // Prevent recursion.
                continue;
            }

            seen.insert(func_id);
            let func = &folder.function_cache[&func_id];
            func.gather_stacks_recursive(key, seen, folder, occurrences);
            seen.remove(&func_id);
        }

        key.truncate(old_prefix_len);
    }
}

impl Call {
    /// Get the function identifier of this call.
    fn get_function_id(&self) -> usize {
        match self {
            Call::WithPath(i, _) => *i,
            Call::WithoutPath(i) => *i,
        }
    }

    /// Get the call as a formatted string, containing either just the function
    /// name, or the function name and the filename, depending on
    /// [Options::include_filenames].
    fn as_str<'f>(&self, folder: &'f Folder) -> Cow<'f, String> {
        match self {
            Call::WithPath(func, file) => {
                let (name, path) = (&folder.function_names[func], &folder.filenames[file]);
                if folder.options.include_filenames {
                    Cow::Owned(format!("{} ({})", name, path.as_deref().unwrap()))
                } else {
                    Cow::Borrowed(name)
                }
            }
            Call::WithoutPath(func) => Cow::Borrowed(&folder.function_names[func]),
        }
    }
}
