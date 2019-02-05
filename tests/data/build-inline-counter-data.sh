#!/usr/bin/env bash

# -no-pie is currently required so the addresses work with addr2line
gcc -static -no-pie -g inline_counter.c -o inline_counter

perf record --call-graph dwarf ./inline_counter
perf script --no-inline > perf-inline-counter.txt

# Make the path to the binary relative to the project root.
sed 's/(.*\/tests\/data\//(.\/tests\/data\//' perf-inline-counter.txt -i

# Cleanup
rm perf.data
