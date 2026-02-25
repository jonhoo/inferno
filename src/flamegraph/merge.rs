use std::cmp::Ordering;
use std::io;
use std::iter;
use std::mem;

use log::warn;

pub(super) struct Frames<'a> {
    pub(super) frames: Vec<TimedFrame<'a>>,
    pub(super) accumulated_samples: usize,
    pub(super) delta_max: usize,
}

#[derive(Debug, PartialEq)]
pub(super) struct TimedFrame<'a> {
    /// Name of the measured function
    pub(super) function: &'a str,

    /// The depth in the function call for this function
    pub(super) depth: usize,

    /// First sample timestamp for this function
    pub(super) start_time: usize,

    /// Last timestamp for this function.
    ///
    /// Together with `start_time` it defined the amount of time spent in it.
    pub(super) end_time: usize,

    /// Present for delta frames
    ///
    /// Difference among two measured values.
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
    open_frames: &mut Vec<TimedFrame<'a>>,
    closed_frames: &mut Vec<TimedFrame<'a>>,
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
    closed_frames.extend(open_frames.drain(first_different..).map(|mut frame| {
        frame.end_time = acc_samples;
        frame
    }));

    // push the frames new to the current iteration
    if first_different < current_frames.len() {
        let last = current_frames.len().saturating_sub(1);
        let new_open_frames = current_frames[first_different..last]
            .iter()
            .enumerate()
            .map(|(i, function)| TimedFrame {
                function,
                depth: first_different + i,
                start_time: acc_samples,
                delta: delta.clone().and(Some(0)),
                end_time: 0,
            });
        open_frames.extend(new_open_frames);

        if let Some(function) = current_frames.get(last) {
            let frame = TimedFrame {
                function,
                depth: last,
                start_time: acc_samples,
                // For some reason the Perl version does a `+=` for `delta`, but I can't figure out why.
                // See https://github.com/brendangregg/FlameGraph/blob/1b1c6deede9c33c5134c920bdb7a44cc5528e9a7/flamegraph.pl#L588
                delta,
                end_time: 0,
            };

            open_frames.push(frame);
        }
    }
}

/// Group common frames of sorted folded lines and accumulate their measurements.
pub(super) fn frames<'a, I>(lines: I, suppress_sort_check: bool) -> io::Result<Frames<'a>>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut accumulated_samples = 0; // accumulator for all valid samples
    let mut ignored = 0;
    let mut open_frames = Default::default();
    let mut closed_frames = Default::default();
    let mut delta = None;
    let mut delta_max = 1;
    let mut stripped_fractional_samples = false;

    let mut previous: Vec<&'a str> = Vec::new();
    let mut current: Vec<&'a str> = Vec::new();

    for mut line in lines {
        let nsamples = match Sample::parse(line.trim_end(), &mut stripped_fractional_samples) {
            Some((sample, samplesi)) => {
                match sample.delta {
                    Some(sample_delta) => {
                        delta = Some(sample_delta);
                        delta_max = std::cmp::max(sample_delta.unsigned_abs(), delta_max);
                    }
                    None => (),
                };

                line = &line[..samplesi].trim();
                if line.is_empty() {
                    ignored += 1;
                    continue;
                }
                sample.samples
            }
            None => {
                ignored += 1;
                continue;
            }
        };

        let current_trace = line;

        // inject empty first-level stack frame to capture "all"
        current.extend(iter::once("").chain(current_trace.split(';')));

        if !suppress_sort_check {
            let is_sorted = previous
                .iter()
                .zip(current.iter())
                .map(|(prev, curr)| prev.cmp(curr))
                .find(|ord| !ord.is_eq());
            if is_sorted == Some(Ordering::Greater) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unsorted input lines detected",
                ));
            }
        }

        flow(
            &mut open_frames,
            &mut closed_frames,
            &previous,
            &current,
            accumulated_samples,
            delta,
        );

        mem::swap(&mut current, &mut previous);
        current.clear();
        accumulated_samples += nsamples;
    }

    if !previous.is_empty() {
        flow(
            &mut open_frames,
            &mut closed_frames,
            &previous,
            &current,
            accumulated_samples,
            delta,
        );
    }

    if ignored != 0 {
        warn!("Ignored {} lines with invalid format", ignored);
    }

    debug_assert!(
        open_frames.is_empty(),
        "Not all open frames have been consumed",
    );

    Ok(Frames {
        frames: closed_frames,
        accumulated_samples,
        delta_max,
    })
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
