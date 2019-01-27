Inferno is a port of parts of the [flamegraph
toolkit](http://www.brendangregg.com/flamegraphs.html) to Rust, with the
aim of improving the performance of the original flamegraph tools. The
primary focus is on speeding up the `stackcollapse-*` tools that process
output from various profiling tools into the "folded" format expected by
the `flamegraph` plotting tool. So far, the focus has been on parsing
profiling results from
[`perf`](https://perf.wiki.kernel.org/index.php/Main_Page), and
`inferno-collapse-perf` is ~10x faster than `stackcollapse-perf`.

It is developed in part through live coding sessions, which you can find
[on YouTube](https://www.youtube.com/c/JonGjengset). The first video in
the sequence is [here](https://www.youtube.com/watch?v=jTpK-bNZiA4).

# Testing

Run `./test.sh` to verify conformity with the original implementation.

# License

Inferno is licensed under [CDDL
1.0](https://opensource.org/licenses/CDDL-1.0) to conform to the license
[used](https://github.com/brendangregg/FlameGraph/commit/76719a446d6091c88434489cc99d6355c3c3ef41)
by the upstream flamegraph files (see, for example,
[`stackcollapse-perf`](https://github.com/brendangregg/FlameGraph/blob/f857ebc94bfe2a9bfdc4f1536ebacfb7466f69ba/stackcollapse-perf.pl#L44L61)).
