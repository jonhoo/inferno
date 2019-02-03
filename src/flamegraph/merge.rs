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
}

fn flow<'a, LI, TI>(
    tmp: &mut HashMap<Frame<'a>, usize>,
    frames: &mut Vec<TimedFrame<'a>>,
    last: LI,
    this: TI,
    time: usize,
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
        let _ = last.next();
        let _ = this.next();
        shared_depth += 1;
    }

    // TODO: document this..

    for (i, func) in last.enumerate() {
        let key = Frame {
            function: func,
            depth: shared_depth + i,
        };

        //eprintln!("at {} ending frame {:?}", time, key);
        let start_time = tmp.remove(&key).unwrap_or_else(|| {
            unreachable!("did not have start time for {:?}", key);
        });

        let key = TimedFrame {
            location: key,
            start_time,
            end_time: time,
        };
        frames.push(key);
    }

    for (i, func) in this.enumerate() {
        let key = Frame {
            function: func,
            depth: shared_depth + i,
        };
        //eprintln!("stored tmp for time {}: {:?}", time, key);
        if let Some(start_time) = tmp.insert(key, time) {
            unreachable!("start time {} already registered for frame", start_time);
        }
    }
}

pub(super) fn frames(input: &str) -> (Vec<TimedFrame>, usize, usize) {
    let mut time = 0;
    let mut ignored = 0;
    let mut last = "";
    let mut tmp = Default::default();
    let mut frames = Default::default();
    for line in input.lines() {
        let mut line = line.trim();
        if line.is_empty() {
            continue;
        }

        let nsamples = if let Some(samplesi) = line.rfind(' ') {
            let mut samples = &line[(samplesi + 1)..];
            // strip fractional part (if any);
            // foobar 1.klwdjlakdj
            if let Some(doti) = samples.find('.') {
                samples = &samples[..doti];
            }
            match samples.parse::<usize>() {
                Ok(nsamples) => {
                    // remove nsamples part we just parsed from line
                    line = line[..samplesi].trim_end();
                    // give out the sample count
                    nsamples
                }
                Err(_) => {
                    ignored += 1;
                    continue;
                }
            }
        } else {
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
            flow(&mut tmp, &mut frames, None, this, time);
        } else {
            //eprintln!("flow({}, {}, {})", last, stack, time);
            flow(
                &mut tmp,
                &mut frames,
                iter::once("").chain(last.split(';')),
                this,
                time,
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
        );
    }

    (frames, time, ignored)
}
