use super::Collapse;
use crate::collapse::common;
use hashbrown::hash_map::RawEntryMut;
use hashbrown::HashMap;
use log::warn;
use std::fmt;
use std::hash::{BuildHasher, Hash, Hasher};
use std::io::prelude::*;
use std::io::{self, Write};
use std::rc::Rc;
// Ideas: str_stack and colosseum to keep allocations closer together or something.

// Xdebug uses nanoseconds, whereas flamegraph expects seconds, hence a scale
// factor of one million.
const SCALE_FACTOR: f32 = 1_000_000.0;
static CALLS: &[&str] = &["require", "require_once", "include", "include_once"];

const TRACE_START: &str = "TRACE START";
const TRACE_END: &str = "TRACE END";

/// Options for the Xdebug collapser
#[derive(Clone, Debug)]
pub struct Options {
    /// The number of threads to use.
    ///
    /// Default is the number of logical cores on your machine.
    pub nthreads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            nthreads: *common::DEFAULT_NTHREADS,
        }
    }
}

/// A unique key for an interned string.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Str(usize);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Call {
    WithPath(Str, Str),
    WithoutPath(Str),
}

enum Interned<T> {
    Old(T),
    New(T),
}

#[derive(Default)]
struct CallStack {
    strings: HashMap<Rc<str>, usize>,
    interned_string: Vec<Rc<str>>,

    calls: HashMap<Call, usize>,
    interned: Vec<Call>,

    stack: Vec<usize>,
}

struct Frames<'a> {
    calls: &'a CallStack,
    stack: &'a [usize],
}

/// Iterate over tab separated fields.
///
/// This is essentially the same as `str.split('\t').filter(|slice| slice.len() > 0)` but seemingly
/// faster. This probably because we know that the character in question is ascii, while the
/// standard library `StrSearcher` for `char` is not optimized around this case.
struct Fields<'a> {
    line: &'a str,
}

/// The Folder struct
pub struct Folder;

impl From<Options> for Folder {
    fn from(_: Options) -> Self {
        Self
    }
}

impl Collapse for Folder {
    fn collapse<R, W>(&mut self, mut reader: R, mut writer: W) -> io::Result<()>
    where
        R: BufRead,
        W: Write,
    {
        // Timings for all call stacks in total.
        let mut stacks: HashMap<_, f32> = HashMap::new();
        let mut current_stack = CallStack::new();
        let mut prev_start_time = 0.0;
        let mut line = String::new();

        loop {
            if reader.read_line(&mut line)? == 0 {
                break;
            }

            if line.starts_with(TRACE_START) {
                break;
            }

            line.clear();
        }

        loop {
            line.clear();

            if reader.read_line(&mut line)? == 0 {
                break;
            }

            if line.starts_with(TRACE_END) {
                break;
            }

            // Break the line into tab separated tokens.
            let mut parts = Fields::new(&line).skip(2);

            let (is_exit, time) = if let (Some(is_exit), Some(time)) = (parts.next(), parts.next())
            {
                let is_exit = match is_exit {
                    "1" => true,
                    "0" => false,
                    a => panic!("uh oh: {}", a),
                };

                let time = time.parse::<f32>().unwrap();

                (is_exit, time)
            } else {
                continue;
            };

            if is_exit && current_stack.is_empty() {
                warn!("Found function exit without corresponding entrance. Discarding line. Check your input.");
                continue;
            }

            {
                let current = current_stack.current();
                let duration = SCALE_FACTOR * (time - prev_start_time);

                // hash `current` for lookup and insertion.
                let mut hasher = stacks.hasher().build_hasher();
                current.hash(&mut hasher);
                let hash = hasher.finish();

                match stacks
                    .raw_entry_mut()
                    .from_key_hashed_nocheck(hash, current)
                {
                    RawEntryMut::Occupied(mut occ) => *occ.get_mut() += duration,
                    RawEntryMut::Vacant(vacant) => {
                        // `Box<str>` has same hash as `str`.
                        vacant.insert_hashed_nocheck(hash, Box::from(current), duration);
                    }
                }
            }

            if is_exit {
                current_stack.pop();
            } else {
                let _ = parts.next();
                let func_name = parts.next();
                let _ = parts.next();
                let path_name = parts.next();

                if let (Some(func_name), Some(path_name)) = (func_name, path_name) {
                    current_stack.call(func_name, path_name);
                }
            }

            prev_start_time = time;
        }

        for (key, value) in stacks {
            writeln!(writer, "{} {}", current_stack.frames(&key), value)?;
        }

        Ok(())
    }

    fn is_applicable(&mut self, input: &str) -> Option<bool> {
        let mut input = input.as_bytes();
        let mut line = String::new();
        loop {
            line.clear();

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

impl CallStack {
    /// Create a function name interning call stack tracker.
    ///
    /// Populated with the constant builtins for inclusion, to enable a faster comparison.
    pub fn new() -> Self {
        CallStack {
            strings: CALLS
                .iter()
                .enumerate()
                .map(|(idx, name)| (name.to_owned().into(), idx))
                .collect(),
            interned_string: CALLS.iter().cloned().map(Rc::from).collect(),
            calls: HashMap::new(),
            interned: Vec::new(),
            stack: Vec::with_capacity(16),
        }
    }

    /// Push a `call` line on top of the stack.
    pub fn call(&mut self, name: &str, path: &str) {
        let new_or_not = match self.intern_str(name) {
            Interned::Old(st @ Str(0..=4)) => match self.intern_str(path) {
                Interned::Old(other) => Interned::Old(Call::WithPath(st, other)),
                Interned::New(new) => Interned::New(Call::WithPath(st, new)),
            },
            Interned::Old(other) => Interned::Old(Call::WithoutPath(other)),
            Interned::New(new) => Interned::New(Call::WithoutPath(new)),
        };

        let idx = self.intern(new_or_not);
        self.stack.push(idx)
    }

    /// Pop one call from the current stack.
    pub fn pop(&mut self) {
        self.stack.pop();
    }

    /// An empty stack means we finished main.
    fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Get a comparable, hashable representation of the call stack. of the call stack.
    fn current(&self) -> &[usize] {
        self.stack.as_slice()
    }

    /// Intern a string, return the unique index.
    fn intern_str(&mut self, string: &str) -> Interned<Str> {
        let mut hasher = self.strings.hasher().build_hasher();
        string.hash(&mut hasher);
        let hash = hasher.finish();

        let entry = self
            .strings
            .raw_entry_mut()
            .from_key_hashed_nocheck(hash, string);

        let vacant = match entry {
            RawEntryMut::Occupied(occ) => return Interned::Old(Str(*occ.get())),
            RawEntryMut::Vacant(vacant) => vacant,
        };

        let index = self.interned_string.len();
        let element: Rc<str> = Rc::from(string);
        self.interned_string.push(element.clone());
        vacant.insert_hashed_nocheck(hash, element, index);
        Interned::New(Str(index))
    }

    /// Intern a `Call` into a `usize` index.
    fn intern(&mut self, call: Interned<Call>) -> usize {
        let new = match call {
            // The strings were not seen before, definitely new.
            Interned::New(t) => t,
            // The strings used were seen before, but maybe not in this call. So retest.
            Interned::Old(t) => {
                if let Some(idx) = self.calls.get(&t) {
                    return *idx;
                } else {
                    t
                }
            }
        };

        let index = self.interned.len();
        self.interned.push(new);
        self.calls.insert(new, index);
        index
    }

    /// Prepare the stack frame for printing.
    fn frames<'a>(&'a self, stack: &'a [usize]) -> Frames<'a> {
        Frames { calls: self, stack }
    }

    /// Create a name for the current stack.
    ///
    /// This is potentially costly.
    fn write_name(&self, indices: &[usize], buffer: &mut fmt::Formatter) -> fmt::Result {
        let mut indices = indices.iter().cloned();
        if let Some(first) = indices.by_ref().next() {
            self.write_call(self.interned[first], buffer)?;
        }
        for next in indices {
            buffer.write_str(";")?;
            self.write_call(self.interned[next], buffer)?;
        }
        Ok(())
    }

    /// Format a single call.
    fn write_call(&self, call: Call, buffer: &mut fmt::Formatter) -> fmt::Result {
        match call {
            Call::WithoutPath(Str(idx)) => buffer.write_str(&self.interned_string[idx]),
            Call::WithPath(Str(name), Str(path)) => {
                let (name, path) = (&self.interned_string[name], &self.interned_string[path]);
                buffer.write_str(name)?;
                buffer.write_str("(")?;
                buffer.write_str(path)?;
                buffer.write_str(")")
            }
        }
    }
}

impl<'a> fmt::Display for Frames<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.calls.write_name(self.stack, f)
    }
}

impl<'a> Fields<'a> {
    pub fn new(line: &'a str) -> Self {
        Fields { line }
    }
}

impl<'a> Iterator for Fields<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        let begin = self
            .line
            .as_bytes()
            .iter()
            .cloned()
            .position(|b| b != b'\t')
            .unwrap_or(self.line.len());

        self.line = &self.line[begin..];
        if self.line.is_empty() {
            return None;
        }

        let end = self
            .line
            .as_bytes()
            .iter()
            .cloned()
            .position(|b| b == b'\t')
            .unwrap_or(self.line.len());
        let (result, next) = self.line.split_at(end);
        self.line = next;
        Some(result)
    }
}
