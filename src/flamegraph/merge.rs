use std::collections::HashMap;
use std::io;
use std::iter;
use std::ops::Add;
use std::ops::AddAssign;

use log::warn;

use crate::flamegraph::FrameWidthSource;

#[derive(Debug, PartialEq, Eq, Hash)]
pub(super) struct Frame<'a> {
    pub(super) function: &'a str,
    pub(super) depth: usize,
}

// Trait alias workaround
pub trait CountTypeRequirements:
    Default + Copy + std::ops::Add<Output = Self> + std::fmt::Debug
{
}
impl<T> CountTypeRequirements for T where
    T: Default + Copy + std::ops::Add<Output = Self> + std::fmt::Debug
{
}

#[derive(Debug, PartialEq)]
pub(super) struct TimedFrame<'a, CountType> {
    pub(super) location: Frame<'a>,
    pub(super) start_time: StackSampleCount<CountType>,
    pub(super) end_time: StackSampleCount<CountType>,
    pub(super) self_and_total_sample_counts: FrameSelfAndTotalCounts<CountType>,
}

impl<'a, CountType> TimedFrame<'a, CountType>
where
    StackSampleCount<CountType>: StackSampleCountExt,
{
    pub(super) fn visual_samples(&self) -> usize {
        self.end_time.visual() - self.start_time.visual()
    }
    pub(super) fn visual_width(&self, overall_total_samples: StackSampleCount<CountType>) -> f64 {
        let (a, b) = self.visual_start_and_end_pct(overall_total_samples);
        b - a
    }
    pub(super) fn visual_start_and_end_pct(
        &self,
        overall_total_samples: StackSampleCount<CountType>,
    ) -> (f64, f64) {
        let frame_start_visual_pct =
            (self.start_time.visual() as f64 / overall_total_samples.visual() as f64) * 100.0;

        let frame_end_visual_pct =
            (self.end_time.visual() as f64 / overall_total_samples.visual() as f64) * 100.0;

        (frame_start_visual_pct, frame_end_visual_pct)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(super) struct UnclosedFrame<CountType> {
    pub(super) start_time: StackSampleCount<CountType>,
    pub(super) sample_count: FrameSelfAndTotalCounts<CountType>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FrameSelfAndTotalCounts<CountType> {
    pub self_count: CountType,
    pub total_count: CountType,
}
impl<CountType: Clone + Copy> FrameSelfAndTotalCounts<CountType> {
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum FrameSelfAndTotalCountsEnum {
    Single(FrameSelfAndTotalCounts<usize>),
    Diff(FrameSelfAndTotalCounts<DiffCount>),
}

pub trait FrameSelfAndTotalCountsExt {
    fn is_diff(&self) -> bool;
    fn to_diff(&self) -> Option<FrameSelfAndTotalCounts<DiffCount>>;
    fn split(&self) -> FrameSelfAndTotalCountsEnum;
}

impl FrameSelfAndTotalCountsExt for FrameSelfAndTotalCounts<usize> {
    fn is_diff(&self) -> bool {
        false
    }
    fn to_diff(&self) -> Option<FrameSelfAndTotalCounts<DiffCount>> {
        None
    }
    fn split(&self) -> FrameSelfAndTotalCountsEnum {
        FrameSelfAndTotalCountsEnum::Single(*self)
    }
}
impl FrameSelfAndTotalCountsExt for FrameSelfAndTotalCounts<DiffCount> {
    fn is_diff(&self) -> bool {
        true
    }
    fn to_diff(&self) -> Option<FrameSelfAndTotalCounts<DiffCount>> {
        Some(*self)
    }
    fn split(&self) -> FrameSelfAndTotalCountsEnum {
        FrameSelfAndTotalCountsEnum::Diff(*self)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct DiffCount {
    pub before: usize,
    pub after: usize,
    pub visual: usize,
}

impl DiffCount {
    pub(super) fn new(before: usize, after: usize, frame_width_source: FrameWidthSource) -> Self {
        Self {
            before,
            after,
            visual: frame_width_source.apply(before, after),
        }
    }
    pub(super) fn delta(&self) -> isize {
        self.after as isize - self.before as isize
    }
    pub fn delta_pct_pt_assuming_both_datasets_have_the_same_number_of_samples(&self, overall_total_sample_count_after: usize) -> f64 {
        ((self.after as f64 / overall_total_sample_count_after as f64)
            - (self.before as f64 / overall_total_sample_count_after as f64))
            * 100.0
    }

    pub(super) fn delta_pct_pt(&self, overall_total_count: DiffCount) -> f64 {
        ((self.after as f64 / overall_total_count.after as f64)
            - (self.before as f64 / overall_total_count.before as f64))
            * 100.0
    }
}

impl Add for DiffCount {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        DiffCount {
            before: self.before + rhs.before,
            after: self.after + rhs.after,
            visual: self.visual + rhs.visual,
        }
    }
}

impl<CountType: Add<Output = CountType>> Add<StackSampleCount<CountType>>
    for FrameSelfAndTotalCounts<CountType>
{
    type Output = Self;

    fn add(self, rhs: StackSampleCount<CountType>) -> Self::Output {
        Self {
            self_count: self.self_count,
            total_count: self.total_count + rhs.0,
        }
    }
}

impl<CountType: Add<Output = CountType> + Copy> AddAssign<StackSampleCount<CountType>>
    for FrameSelfAndTotalCounts<CountType>
{
    fn add_assign(&mut self, rhs: StackSampleCount<CountType>) {
        *self = *self + rhs;
    }
}

impl FrameSelfAndTotalCounts<DiffCount> {
    pub(super) fn delta(&self, include_children: bool) -> Option<isize> {
        Some(if include_children {
            self.total_count.delta()
        } else {
            self.self_count.delta()
        })
    }
    pub(super) fn normalized_delta(
        &self,
        include_children: bool,
        overall_sample_count: DiffCount,
    ) -> Option<f32> {
        let frame_count = if include_children {
            self.total_count
        } else {
            self.self_count
        };

        Some(
            frame_count.after as f32 / overall_sample_count.after as f32
                - frame_count.before as f32 / overall_sample_count.before as f32,
        )
    }
    pub(super) fn total_delta(&self) -> Option<isize> {
        self.delta(true)
    }
    pub(super) fn self_delta(&self) -> Option<isize> {
        self.delta(false)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct StackSampleCount<CountType>(pub CountType);

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum StackSampleCountEnum {
    Single(usize),
    Diff(DiffCount),
}

pub trait StackSampleCountExt: Default {
    fn delta(&self) -> Option<isize>;
    fn visual(&self) -> usize;
    fn is_diff(&self) -> bool;
    fn to_diff(&self) -> Option<DiffCount>;
    fn parse_from_line(
        line: &mut &str,
        stripped_fractional_samples: &mut bool,
        frame_width_source: FrameWidthSource,
    ) -> Option<Self>;
    fn last_count(&self) -> usize;
    fn split(&self) -> StackSampleCountEnum;
}

impl<T> Default for StackSampleCount<T>
where
    T: Default,
{
    fn default() -> Self {
        Self(Default::default())
    }
}

impl StackSampleCountExt for StackSampleCount<usize> {
    fn delta(&self) -> Option<isize> {
        None
    }
    fn visual(&self) -> usize {
        self.0
    }
    fn is_diff(&self) -> bool {
        false
    }
    fn to_diff(&self) -> Option<DiffCount> {
        None
    }
    fn parse_from_line(
        mut line: &mut &str,
        mut stripped_fractional_samples: &mut bool,
        _frame_width_source: FrameWidthSource,
    ) -> Option<Self> {
        let Some(col1) = parse_nsamples(&mut line, &mut stripped_fractional_samples) else {
            return None;
        };
        Some(StackSampleCount(col1))
    }
    fn last_count(&self) -> usize {
        self.0
    }
    fn split(&self) -> StackSampleCountEnum {
        StackSampleCountEnum::Single(self.0)
    }
}
impl StackSampleCountExt for StackSampleCount<DiffCount> {
    fn delta(&self) -> Option<isize> {
        Some(self.0.after as isize - self.0.before as isize)
    }
    fn visual(&self) -> usize {
        self.0.visual
    }
    fn is_diff(&self) -> bool {
        true
    }
    fn to_diff(&self) -> Option<DiffCount> {
        Some(self.0)
    }
    fn parse_from_line(
        line: &mut &str,
        stripped_fractional_samples: &mut bool,
        frame_width_source: FrameWidthSource,
    ) -> Option<Self> {
        let Some(col2) = parse_nsamples(line, stripped_fractional_samples) else {
            return None;
        };
        let Some(col1) = parse_nsamples(line, stripped_fractional_samples) else {
            return None;
        };
        Some(StackSampleCount(DiffCount::new(
            col1,
            col2,
            frame_width_source,
        )))
    }
    fn last_count(&self) -> usize {
        self.0.after
    }
    fn split(&self) -> StackSampleCountEnum {
        StackSampleCountEnum::Diff(self.0)
    }
}

impl<CountType: Add<Output = CountType>> Add for StackSampleCount<CountType> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}
impl<CountType> AddAssign for StackSampleCount<CountType>
where
    Self: std::ops::Add<Output = Self> + Copy,
{
    fn add_assign(&mut self, rhs: StackSampleCount<CountType>) {
        *self = *self + rhs;
    }
}

#[derive(Debug, Default)]
pub(super) struct MaxAbsDelta {
    pub(super) max_abs_self_delta: usize,
    pub(super) max_abs_total_delta: usize,

    pub(super) max_abs_self_delta_pct_pt: f64,
    pub(super) max_abs_total_delta_pct_pt: f64,
}
impl MaxAbsDelta {
    fn fmax(a: f64, b: f64) -> f64 {
        if a > b {
            a
        } else {
            b
        }
    }
    pub(super) fn elementwise_max(&self, rhs: Self) -> Self {
        MaxAbsDelta {
            max_abs_self_delta: std::cmp::max(self.max_abs_self_delta, rhs.max_abs_self_delta),
            max_abs_total_delta: std::cmp::max(self.max_abs_total_delta, rhs.max_abs_total_delta),
            max_abs_self_delta_pct_pt: Self::fmax(
                self.max_abs_self_delta_pct_pt,
                rhs.max_abs_self_delta_pct_pt,
            ),
            max_abs_total_delta_pct_pt: Self::fmax(
                self.max_abs_total_delta_pct_pt,
                rhs.max_abs_total_delta_pct_pt,
            ),
        }
    }
}

fn flow<'a, LI, TI, CountType>(
    open_raw_frames: &mut HashMap<Frame<'a>, UnclosedFrame<CountType>>,
    closed_frames: &mut Vec<TimedFrame<'a, CountType>>,
    last: LI,
    this: TI,
    total_sample_count_so_far: Option<StackSampleCount<CountType>>,
    sample_count_for_this_line: Option<StackSampleCount<CountType>>,
) where
    LI: IntoIterator<Item = &'a str>,
    TI: IntoIterator<Item = &'a str>,
    CountType: CountTypeRequirements,
{
    let mut this = this.into_iter().peekable();
    let mut last = last.into_iter().peekable();

    let accumulated_samples = match total_sample_count_so_far {
        None => Some(Default::default()),
        x => x,
    };
    // remove common prefix
    let mut shared_depth = 0;
    while last.peek() == this.peek() {
        // they must both be None, so let's stop looping
        if last.peek().is_none() {
            break;
        }

        let key = Frame {
            function: this.peek().unwrap(),
            depth: shared_depth,
        };
        open_raw_frames
            .get_mut(&key)
            .map(|frame_time| frame_time.sample_count += sample_count_for_this_line.unwrap());

        // move along prefix iterators
        last.next();
        this.next();
        shared_depth += 1;
    }

    // TODO: document this..

    // Closing off frames
    for (i, func) in last.enumerate() {
        let key = Frame {
            function: func,
            depth: shared_depth + i,
        };

        //eprintln!("at {} ending frame {:?}", time, key);
        let unclosed_frame = open_raw_frames.remove(&key).unwrap_or_else(|| {
            unreachable!("did not have start time for {:?}", key);
        });

        let frame = TimedFrame {
            location: key,
            start_time: unclosed_frame.start_time,
            end_time: accumulated_samples.unwrap(),
            self_and_total_sample_counts: unclosed_frame.sample_count,
        };
        // TODO: delete
        // if frame.self_and_total_sample_counts.is_diff() && i != 0 {
        //     let total_delta = frame.self_and_total_sample_counts.total_delta().unwrap();
        //     let self_delta = frame.self_and_total_sample_counts.self_delta().unwrap();
        // }
        closed_frames.push(frame);
    }

    // Opening new frames
    let mut i = 0;
    while this.peek().is_some() {
        let func = this.next().unwrap();
        let key = Frame {
            function: func,
            depth: shared_depth + i,
        };

        let is_last_frame_of_stack = this.peek().is_none();

        let frame_time = UnclosedFrame {
            start_time: accumulated_samples.unwrap(),
            // For some reason the Perl version does a `+=` for `delta`, but I can't figure out why.
            // See https://github.com/brendangregg/FlameGraph/blob/1b1c6deede9c33c5134c920bdb7a44cc5528e9a7/flamegraph.pl#L588
            sample_count: FrameSelfAndTotalCounts {
                self_count: if is_last_frame_of_stack {
                    sample_count_for_this_line.unwrap().0
                } else {
                    Default::default()
                },
                total_count: sample_count_for_this_line.unwrap().0,
            },
        };

        //eprintln!("stored tmp for time {}: {:?}", time, key);
        if let Some(frame_time) = open_raw_frames.insert(key, frame_time) {
            unreachable!(
                "start time {:?} already registered for frame",
                frame_time.start_time
            );
        }

        i += 1;
    }
}

pub(super) fn frames<'a, I, CountType>(
    lines: I,
    suppress_sort_check: bool,
    frame_width_source: FrameWidthSource,
) -> io::Result<(
    Vec<TimedFrame<'a, CountType>>,
    Option<StackSampleCount<CountType>>,
    usize,
    Option<MaxAbsDelta>,
)>
where
    I: IntoIterator<Item = &'a str>,
    CountType: CountTypeRequirements,
    StackSampleCount<CountType>: StackSampleCountExt,
    FrameSelfAndTotalCounts<CountType>: FrameSelfAndTotalCountsExt,
{
    let mut sample_count_before_this_line = None;
    let mut ignored = 0;
    let mut last = "";
    let mut open_frames = Default::default();
    let mut closed_frames = Default::default();
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

        // Parse the number of samples for the purpose of computing overall time passed.
        // Usually there will only be one samples column at the end of a line,
        // but for differentials there will be two. When there are two we compute the
        // delta between them and use the second one.
        let Some(sample_count_for_this_line) = StackSampleCount::parse_from_line(
            &mut line,
            &mut stripped_fractional_samples,
            frame_width_source,
        ) else {
            ignored += 1;
            continue;
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
            //eprintln!("flow(_, {}, {})", stack, time);
            flow(
                &mut open_frames,
                &mut closed_frames,
                None,
                this,
                sample_count_before_this_line,
                Some(sample_count_for_this_line),
            );
        } else {
            //eprintln!("flow({}, {}, {})", last, stack, time);
            flow(
                &mut open_frames,
                &mut closed_frames,
                iter::once("").chain(last.split(';')),
                this,
                sample_count_before_this_line,
                Some(sample_count_for_this_line),
            );
        }

        last = stack;
        sample_count_before_this_line = match sample_count_before_this_line {
            Some(x) => Some(x + sample_count_for_this_line),
            None => Some(sample_count_for_this_line),
        };
        prev_line = Some(line);
    }

    // Close off remaining open frames at the end
    if !last.is_empty() {
        //eprintln!("flow({}, _, {})", last, time);
        flow(
            &mut open_frames,
            &mut closed_frames,
            iter::once("").chain(last.split(';')),
            None,
            sample_count_before_this_line,
            None,
        );
    }

    assert!(open_frames.is_empty());

    // Iterate through all frames a second time to calculate percentage diffs, and associated percentage point max diffs
    let maybe_delta_max = if let Some(total_sample_count) = sample_count_before_this_line {
        max_deltas(&closed_frames, total_sample_count)
    } else {
        None
    };

    Ok((
        closed_frames,
        sample_count_before_this_line,
        ignored,
        maybe_delta_max,
    ))
}

fn max_deltas<CountType: CountTypeRequirements>(
    frames: &[TimedFrame<CountType>],
    overall_total_samples: StackSampleCount<CountType>,
) -> Option<MaxAbsDelta>
where
    StackSampleCount<CountType>: StackSampleCountExt,
    FrameSelfAndTotalCounts<CountType>: FrameSelfAndTotalCountsExt,
{
    if !overall_total_samples.is_diff() {
        return None;
    };
    Some(frames.iter().fold(
        Default::default(),
        |max_abs_delta: MaxAbsDelta, frame: &TimedFrame<CountType>| {
            max_abs_delta.elementwise_max(MaxAbsDelta {
                max_abs_self_delta: frame
                    .self_and_total_sample_counts
                    .to_diff()
                    .unwrap()
                    .self_delta()
                    .unwrap()
                    .unsigned_abs(),
                max_abs_total_delta: frame
                    .self_and_total_sample_counts
                    .to_diff()
                    .unwrap()
                    .total_delta()
                    .unwrap()
                    .unsigned_abs(),
                max_abs_self_delta_pct_pt: frame
                    .self_and_total_sample_counts
                    .to_diff()
                    .unwrap()
                    .self_count
                    .delta_pct_pt(overall_total_samples.to_diff().unwrap())
                    .abs(),
                max_abs_total_delta_pct_pt: frame
                    .self_and_total_sample_counts
                    .to_diff()
                    .unwrap()
                    .total_count
                    .delta_pct_pt(overall_total_samples.to_diff().unwrap())
                    .abs(),
            })
        },
    ))
}

// Parse and remove the number of samples from the end of a line.
fn parse_nsamples(line: &mut &str, stripped_fractional_samples: &mut bool) -> Option<usize> {
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
        *line = line[..samplesi].trim_end();
        Some(nsamples)
    } else {
        None
    }
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
