#!/bin/bash
#
# test.sh - Check flame graph software vs test result files.
#
# This is used to detect regressions in the flame graph software.
# See record-test.sh, which refreshes these files after intended software
# changes.
#
# Currently only tests inferno (for stack collapsing)

set -euo pipefail
set -x
set -v

for opt in pid tid kernel jit all addrs; do
  for testfile in test/*.txt ; do
    echo testing $testfile : $opt
    outfile=${testfile#*/}
    outfile=test/results/${outfile%.txt}"-collapsed-${opt}.txt"
    cargo run -- --"${opt}" "${testfile}" 2> /dev/null | diff -u - "${outfile}"
    # perl ./flamegraph.pl "${outfile}" > /dev/null
  done
done
