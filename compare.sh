#!/usr/bin/env bash

set -eu -o pipefail

cargo build --release --bin inferno-collapse-perf
BIN="${CARGO_TARGET_DIR:-target}/release/inferno-collapse-perf"

(( maxsize = 100 * 1024 ))
for f in ./flamegraph/test/*.txt; do
	# only run benchmark on larger files
	filesize=$(stat -c%s "$f")
	if (( filesize > maxsize )); then
		echo "==>  $f  <=="
		hyperfine "$BIN --all $f" "./flamegraph/stackcollapse-perf.pl --all $f"
		echo
		echo
	fi
done
