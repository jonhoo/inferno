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

fn flow<'a>(
    tmp: &mut HashMap<Frame<'a>, FrameTime>,
    frames: &mut Vec<TimedFrame<'a>>,
    previous_frames: &[&'a str],
    current_frames: &[&'a str],
    acc_samples: usize,
    delta: Option<isize>,
) {
    debug_assert!(
        previous_frames.len() > 0 || current_frames.len() > 0,
        "At least one of the frames must be non-empty",
    );

    // find common prefix among previous and current frames
    let mut first_different = 0;
    let max_depth = previous_frames.len().min(current_frames.len());
    while first_different < max_depth {
        if previous_frames[first_different] != current_frames[first_different] {
            break;
        }
        first_different += 1;
    }

    // remove the frames from the last iteration which are not shared with the
    // current
    let mut depth = first_different;
    while depth < previous_frames.len() {
        let key = Frame {
            function: previous_frames[depth],
            depth,
        };

        // the previous value was processed on the previous iteration, so this
        // value must exist
        let frame_time = tmp.remove(&key).unwrap_or_else(|| {
            unreachable!("did not have start time for {:?}", key);
        });

        let frame = TimedFrame {
            location: key,
            start_time: frame_time.start_time,
            end_time: acc_samples,
            delta: frame_time.delta,
        };
        frames.push(frame);

        depth += 1;
    }

    // push the frames new to the current iteration
    let mut depth = first_different;
    let inner_delta = delta.clone().and(Some(0)); // None and Some(0) render differently
    while depth < current_frames.len().saturating_sub(1) {
        let key = Frame {
            function: current_frames[depth],
            depth,
        };

        let frame_time = FrameTime {
            start_time: acc_samples,
            delta: inner_delta.clone(),
        };

        let previous = tmp.insert(key, frame_time);
        debug_assert!(
            previous.is_none(),
            "Unexpected frame key. This key should be unique to current value",
        );

        depth += 1;
    }

    if depth < current_frames.len() {
        let key = Frame {
            function: current_frames[depth],
            depth,
        };

        let frame_time = FrameTime {
            start_time: acc_samples,
            // For some reason the Perl version does a `+=` for `delta`, but I can't figure out why.
            // See https://github.com/brendangregg/FlameGraph/blob/1b1c6deede9c33c5134c920bdb7a44cc5528e9a7/flamegraph.pl#L588
            delta,
        };

        let previous = tmp.insert(key, frame_time);
        debug_assert!(
            previous.is_none(),
            "Unexpected frame key. This key should be unique to current value",
        );
    }
}

pub(super) fn frames<'a, I>(
    lines: I,
    suppress_sort_check: bool,
) -> io::Result<(Vec<TimedFrame<'a>>, usize, usize)>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut acc_samples = 0; // accumulator for all valid samples
    let mut ignored = 0;
    let mut previous_trace = "";
    let mut tmp = Default::default();
    let mut timed_frames = Default::default(); // compute timmings for a frame
    let mut delta = None;
    let mut delta_max = 1;
    let mut stripped_fractional_samples = false;
    let mut prev_line = None;
    let mut previous = smallvec::SmallVec::<[&str; 6]>::new();

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
        let current_trace = line;

        // inject empty first-level stack frame to capture "all"
        let current = smallvec::SmallVec::<[&str; 6]>::from_iter(
            iter::once("").chain(current_trace.split(';')),
        );

        flow(
            &mut tmp,
            &mut timed_frames,
            &previous,
            &current,
            acc_samples,
            delta,
        );

        previous_trace = current_trace;
        previous = current;
        acc_samples += nsamples;
        prev_line = Some(line);
    }

    if !previous_trace.is_empty() {
        let current = smallvec::SmallVec::<[&str; 6]>::new();
        flow(
            &mut tmp,
            &mut timed_frames,
            &previous,
            &current,
            acc_samples,
            delta,
        );
    }

    if ignored != 0 {
        warn!("Ignored {} lines with invalid format", ignored);
    }

    Ok((timed_frames, acc_samples, delta_max))
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
