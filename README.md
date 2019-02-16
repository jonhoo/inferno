[![Crates.io](https://img.shields.io/crates/v/inferno.svg)](https://crates.io/crates/inferno)
[![Documentation](https://docs.rs/inferno/badge.svg)](https://docs.rs/inferno/)
[![Build Status](https://travis-ci.org/jonhoo/inferno.svg?branch=master)](https://travis-ci.org/jonhoo/inferno)
[![Codecov](https://codecov.io/github/jonhoo/inferno/coverage.svg?branch=master)](https://codecov.io/gh/jonhoo/inferno)
[![Dependency status](https://deps.rs/repo/github/jonhoo/inferno/status.svg)](https://deps.rs/repo/github/jonhoo/inferno)

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

# Dependency

You need to have the [`perf`](https://perf.wiki.kernel.org/index.php/Main_Page) tool installed on your Linux systems.
This can involve installing package like `linux-tools-generic` for Ubuntu or `linux-tools` for Debian.
You may need to tweak a kernel config such as `echo 0 | sudo tee /proc/sys/kernel/perf_event_paranoid`, see [this stackoverflow answer](https://unix.stackexchange.com/a/14256) for details.

# How to Use

Build Inferno
```
cargo build --release
```

Run a program using profiling with perf
```
perf record -g [your program]
```

Transform perf output to svg
```
perf script | ./target/release/inferno-collapse-perf | ./target/release/inferno-flamegraph > out.svg
```

# Comparison to the Perl implementation

To compare performance, run `./compare.sh`. It requires [hyperfine](https://github.com/sharkdp/hyperfine).

# License

Inferno is a port of @brendangregg's awesome original
[FlameGraph](https://github.com/brendangregg/FlameGraph) project,
written in Perl, and owes its existence and pretty much of all of its
functionality entirely to that project. [Like
FlameGraph](https://github.com/brendangregg/FlameGraph/commit/76719a446d6091c88434489cc99d6355c3c3ef41),
Inferno is licensed under the [CDDL
1.0](https://opensource.org/licenses/CDDL-1.0) to avoid any licensing
issues. Specifically, the CDDL 1.0 grants

> a world-wide, royalty-free, non-exclusive license under intellectual
> property rights (other than patent or trademark) Licensable by Initial
> Developer, to use, reproduce, modify, display, perform, sublicense and
> distribute the Original Software (or portions thereof), with or
> without Modifications, and/or as part of a Larger Work; and under
> Patent Claims infringed by the making, using or selling of Original
> Software, to make, have made, use, practice, sell, and offer for sale,
> and/or otherwise dispose of the Original Software (or portions
> thereof).

as long as the source is made available along with the license (3.1),
both of which are true since you're reading this file!
