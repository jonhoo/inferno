#!/usr/bin/env bash

# -no-pie is currently required so the addresses work with addr2line
gcc -static -no-pie -g inline-counter.c -o inline-counter

perf record --call-graph dwarf ./inline-counter
perf script --no-inline > perf-inline-counter.txt

# Make the path to the binary relative to the project root.
sed 's/(.*\/tests\/inline-counter\/data\//(.\/tests\/inline-counter\/data\//' perf-inline-counter.txt -i

# Cleanup
rm perf.data
