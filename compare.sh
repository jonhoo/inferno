#!/usr/bin/env bash

set -eu -o pipefail
BIN="${CARGO_TARGET_DIR:-target}/release/"


cargo build --release --bin inferno-collapse-perf
f=flamegraph/example-perf-stacks.txt
zcat flamegraph/example-perf-stacks.txt.gz > "$f"
echo "==>  perf  <=="
hyperfine --warmup 20 -m 50 "$BIN/inferno-collapse-perf --all $f" "./flamegraph/stackcollapse-perf.pl --all $f"
rm "$f"

echo
echo

cargo build --release --bin inferno-collapse-dtrace
f=flamegraph/example-dtrace-stacks.txt
echo "==>  dtrace  <=="
hyperfine --warmup 20 -m 50 "$BIN/inferno-collapse-dtrace $f" "./flamegraph/stackcollapse.pl $f"
