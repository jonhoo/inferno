use std::collections::HashMap;
use std::iter;

#[derive(Debug, PartialEq, Eq, Hash)]
pub(super) struct Frame<'a> {
    pub(super) function: &'a str,
    pub(super) depth: usize,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub(super) struct TimedFrame<'a> {
    pub(super) location: Frame<'a>,
    pub(super) start_time: usize,
    pub(super) end_time: usize,
    pub(super) delta: Option<isize>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct FrameTime {
    pub(super) start_time: usize,
    pub(super) delta: Option<isize>,
}

fn flow<'a, LI, TI>(
    tmp: &mut HashMap<Frame<'a>, FrameTime>,
    frames: &mut Vec<TimedFrame<'a>>,
    last: LI,
    this: TI,
    time: usize,
    delta: Option<isize>,
) where
    LI: IntoIterator<Item = &'a str>,
    TI: IntoIterator<Item = &'a str>,
{
    let mut this = this.into_iter().peekable();
    let mut last = last.into_iter().peekable();

    // remove common prefix
    let mut shared_depth = 0;
    while last.peek() == this.peek() {
        // they must both be None, so let's stop looping
        if last.peek().is_none() {
            break;
        }

        // move along prefix iterators
        last.next();
        this.next();
        shared_depth += 1;
    }

    // TODO: document this..

    for (i, func) in last.enumerate() {
        let key = Frame {
            function: func,
            depth: shared_depth + i,
        };

        //eprintln!("at {} ending frame {:?}", time, key);
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

    let mut i = 0;
    while this.peek().is_some() {
        let func = this.next().unwrap();
        let key = Frame {
            function: func,
            depth: shared_depth + i,
        };

        let is_last = this.peek().is_none();
        let delta = match delta {
            Some(_) if !is_last => Some(0),
            d => d,
        };
        let frame_time = FrameTime {
            start_time: time,
            delta,
        };

        //eprintln!("stored tmp for time {}: {:?}", time, key);
        if let Some(frame_time) = tmp.insert(key, frame_time) {
            unreachable!(
                "start time {} already registered for frame",
                frame_time.start_time
            );
        }

        i += 1;
    }
}

pub(super) fn frames<'a, I>(lines: I) -> (Vec<TimedFrame<'a>>, usize, usize, usize)
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
    for line in lines {
        let mut line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Usually there will only be one samples column at the end of a line,
        // but for differentials there will be two. In the case there are two
        // we compute the delta between them and use the second one as as the
        // number of samples for this frame.
        let nsamples = match (parse_nsamples(&mut line), parse_nsamples(&mut line)) {
            (Some(s2), nsamples1) => {
                delta = nsamples1.and_then(|s1| Some(s2 as isize - s1 as isize));
                if let Some(delta) = delta {
                    delta_max = std::cmp::max(delta.abs() as usize, delta_max);
                }
                s2
            }
            _ => {
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
            //eprintln!("flow(_, {}, {})", stack, time);
            flow(&mut tmp, &mut frames, None, this, time, delta);
        } else {
            //eprintln!("flow({}, {}, {})", last, stack, time);
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
    }

    if !last.is_empty() {
        //eprintln!("flow({}, _, {})", last, time);
        flow(
            &mut tmp,
            &mut frames,
            iter::once("").chain(last.split(';')),
            None,
            time,
            delta,
        );
    }

    (frames, time, ignored, delta_max)
}

// Parse and remove the number of samples from the end of a line.
fn parse_nsamples(line: &mut &str) -> Option<usize> {
    if let Some(samplesi) = line.rfind(' ') {
        let mut samples = &line[(samplesi + 1)..];
        // strip fractional part (if any);
        // foobar 1.klwdjlakdj
        // TODO: Properly handle fractional samples (see issue #43)
        if let Some(doti) = samples.find('.') {
            samples = &samples[..doti];
        }
        if let Ok(nsamples) = samples.parse::<usize>() {
            // remove nsamples part we just parsed from line
            *line = line[..samplesi].trim_end();
            // give out the sample count
            return Some(nsamples);
        }
    }

    None
}
