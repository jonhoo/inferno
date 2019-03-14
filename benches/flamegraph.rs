extern crate criterion;
extern crate inferno;

use criterion::*;
use inferno::flamegraph;
use std::fs::File;
use std::io::{BufReader, Cursor, Read};

fn flamegraph_benchmark(c: &mut Criterion) {
    let mut f = File::open("tests/data/collapse-perf/results/example-perf-stacks-collapsed.txt")
        .expect("file not found");

    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).expect("Could not read file");

    let len = bytes.len();
    c.bench(
        "flamegraph",
        ParameterizedBenchmark::new(
            "example-perf-stacks-collapsed",
            move |b, data| {
                b.iter(|| {
                    let reader = BufReader::new(data.as_slice());
                    let mut result = Cursor::new(Vec::with_capacity(len));
                    result.set_position(0);
                    let _folder =
                        flamegraph::from_reader(&mut Default::default(), reader, &mut result);
                })
            },
            vec![bytes],
        )
        .throughput(|bytes| Throughput::Bytes(bytes.len() as u32)),
    );
}

criterion_group!(benches, flamegraph_benchmark);
criterion_main!(benches);
