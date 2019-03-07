#!/usr/bin/env bash

# -no-pie is currently required so the addresses work with addr2line
gcc -static -no-pie -g inline-counter.c -o inline-counter

perf record --call-graph dwarf ./inline-counter
perf script --no-inline > ../inline-counter.txt

# Make the path to the binary relative to the project root.
sed 's/(.*\/tests\/data\/collapse-perf\/inline-counter/(.\/tests\/data\/collapse-perf\/inline-counter/' ../inline-counter.txt -i

# Cleanup
rm perf.data
