#!/usr/bin/env bash

set -eu -o pipefail
set -x

[[ -d ./FlameGraph ]] || git clone https://github.com/brendangregg/FlameGraph &
cargo build --release &

wait
hyperfine './target/release/inferno-collapse-perf --all test/perf-iperf-stacks-pidtid-01.txt' './FlameGraph/stackcollapse-perf.pl --all test/perf-iperf-stacks-pidtid-01.txt'

