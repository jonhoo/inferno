use std::collections::HashMap;
use std::io;
use std::iter;

use log::warn;

#[derive(Debug, PartialEq, Eq, Hash)]
pub(super) struct Frame<'a> {
    pub(super) function: &'a str,
    pub(super) depth: usize,
}

#[derive(Debug, PartialEq)]
pub(super) struct TimedFrame<'a> {
    pub(super) location: Frame<'a>,
    pub(super) start_time: usize,
    pub(super) end_time: usize,
    pub(super) delta: Option<isize>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(super) struct FrameTime {
    pub(super) start_time: usize,
    pub(super) delta: Option<isize>,
}

struct Sample {
    samples: usize,
    delta: Option<isize>,
}

impl Sample {
    /// Parse and remove a single sample from the end of a line.
    fn parse_nsamples(
        line: &str,
        stripped_fractional_samples: &mut bool,
    ) -> Option<(usize, usize)> {
        if let Some((samplesi, doti)) = rfind_samples(line) {
            let mut samples = &line[samplesi..];
            // Strip fractional part (if any);
            // foobar 1.klwdjlakdj
            //
            // The Perl version keeps the fractional part but this can be problematic
            // because of cumulative floating point errors. Instead we recommend to
            // use the --factor option. See https://github.com/brendangregg/FlameGraph/pull/18
            //
            // Warn if we're stripping a non-zero fractional part, but only the first time.
            if !*stripped_fractional_samples
                && doti < samples.len() - 1
                && !samples[doti + 1..].chars().all(|c| c == '0')
            {
                *stripped_fractional_samples = true;
                warn!(
                    "The input data has fractional sample counts that will be truncated to integers. \
                     If you need to retain the extra precision you can scale up the sample data and \
                     use the --factor option to scale it back down."
                );
            }
            samples = &samples[..doti];
            let nsamples = samples.parse::<usize>().ok()?;
            // remove nsamples part we just parsed from line
            Some((nsamples, samplesi))
        } else {
            None
        }
    }

    /// Parses the number of samples in a line.
    ///
    /// # Returns
    ///
    /// - None if the line has no sample data.
    /// - Otherwise a sample and the amount of unconsumed data.
    fn parse(line: &str, stripped_fractional_samples: &mut bool) -> Option<(Self, usize)> {
        let (samples, samplei) = Sample::parse_nsamples(line, stripped_fractional_samples)?;

        // handle differential column
        let (delta, samplei) = match Sample::parse_nsamples(
            &line[..samplei].trim_end(),
            stripped_fractional_samples,
        ) {
            Some((original_samples, samplei)) => {
                (Some(samples as isize - original_samples as isize), samplei)
            }
            None => (None, samplei),
        };

        Some((Sample { samples, delta }, samplei))
    }
}

fn flow<'a, LI, TI>(
    tmp: &mut HashMap<Frame<'a>, FrameTime>,
    frames: &mut Vec<TimedFrame<'a>>,
    previous_it: LI,
    current_it: TI,
    time: usize,
    delta: Option<isize>,
) where
    LI: IntoIterator<Item = &'a str>,
    TI: IntoIterator<Item = &'a str>,
{
    let mut current_it = current_it.into_iter().peekable();
    let mut previous_it = previous_it.into_iter().peekable();

    // remove the prefix values shared among the current and previous values
    let mut shared_depth = 0;
    while previous_it.peek() == current_it.peek() {
        // they must both be None, so let's stop looping
        if previous_it.peek().is_none() {
            break;
        }

        // move along prefix iterators
        previous_it.next();
        current_it.next();
        shared_depth += 1;
    }

    // remove the frames from the last iteration which are not shared with the
    // current
    for (i, func) in previous_it.enumerate() {
        let key = Frame {
            function: func,
            depth: shared_depth + i,
        };

        // the previous value was processed on the previous iteration, so this
        // value must exist
        let frame_time = tmp.remove(&key).unwrap_or_else(|| {
            unreachable!("did not have start time for {:?}", key);
        });

        let frame = TimedFrame {
            location: key,
            start_time: frame_time.start_time,
            end_time: time,
            delta: frame_time.delta,
        };
        frames.push(frame);
    }

    // push the frames new to the current iteration
    let mut i = 0;
    while let Some(func) = current_it.next() {
        let key = Frame {
            function: func,
            depth: shared_depth + i,
        };

        let is_last = current_it.peek().is_none();
        let delta = match delta {
            Some(_) if !is_last => Some(0),
            d => d,
        };
        let frame_time = FrameTime {
            start_time: time,
            // For some reason the Perl version does a `+=` for `delta`, but I can't figure out why.
            // See https://github.com/brendangregg/FlameGraph/blob/1b1c6deede9c33c5134c920bdb7a44cc5528e9a7/flamegraph.pl#L588
            delta,
        };

        let previous = tmp.insert(key, frame_time);
        debug_assert!(
            previous.is_none(),
            "Unexpected frame key. This key should be unique to current value",
        );

        i += 1;
    }
}

pub(super) fn frames<'a, I>(
    lines: I,
    suppress_sort_check: bool,
) -> io::Result<(Vec<TimedFrame<'a>>, usize, usize, usize)>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut time = 0;
    let mut ignored = 0;
    let mut last = "";
    let mut tmp = Default::default();
    let mut frames = Default::default();
    let mut delta = None;
    let mut delta_max = 1;
    let mut stripped_fractional_samples = false;
    let mut prev_line = None;
    for line in lines {
        let mut line = line.trim();

        if !suppress_sort_check {
            if let Some(prev_line) = prev_line {
                if prev_line > line {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "unsorted input lines detected",
                    ));
                }
            }
        }

        let nsamples = match Sample::parse(line, &mut stripped_fractional_samples) {
            Some((sample, samplesi)) => {
                match sample.delta {
                    Some(sample_delta) => {
                        delta = Some(sample_delta);
                        delta_max = std::cmp::max(sample_delta.unsigned_abs(), delta_max);
                    }
                    None => (),
                };

                line = &line[..samplesi].trim_end();
                sample.samples
            }
            None => {
                ignored += 1;
                continue;
            }
        };

        if line.is_empty() {
            ignored += 1;
            continue;
        }
        let stack = line;

        // inject empty first-level stack frame to capture "all"
        let this = iter::once("").chain(stack.split(';'));
        if last.is_empty() {
            // need to special-case this, because otherwise iter("") + "".split(';') == ["", ""]
            flow(&mut tmp, &mut frames, None, this, time, delta);
        } else {
            flow(
                &mut tmp,
                &mut frames,
                iter::once("").chain(last.split(';')),
                this,
                time,
                delta,
            );
        }

        last = stack;
        time += nsamples;
        prev_line = Some(line);
    }

    if !last.is_empty() {
        flow(
            &mut tmp,
            &mut frames,
            iter::once("").chain(last.split(';')),
            None,
            time,
            delta,
        );
    }

    Ok((frames, time, ignored, delta_max))
}

// Tries to find a sample count at the end of a line.
//
// On success, the first value of the returned tuple will be the index to the sample count.
// If the sample count is fractional, the second value will be the offset of the dot within
// the sample count.
// If the sample count is not fractional, the second value returned is the offset
// to the last digit in the sample count.
//
// If no sample count is found, `None` will be returned.
pub(super) fn rfind_samples(line: &str) -> Option<(usize, usize)> {
    let samplesi = line.rfind(' ')? + 1;
    let samples = &line[samplesi..];
    if let Some(doti) = samples.find('.') {
        if samples[..doti]
            .chars()
            .chain(samples[doti + 1..].chars())
            .all(|c| c.is_ascii_digit())
        {
            Some((samplesi, doti))
        } else {
            None
        }
    } else if !samples.chars().all(|c| c.is_ascii_digit()) {
        None
    } else {
        Some((samplesi, line.len() - samplesi))
    }
}
