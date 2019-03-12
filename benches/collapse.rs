extern crate criterion;
extern crate inferno;

use criterion::*;
use inferno::collapse::dtrace::{Folder as DFolder, Options as DOptions};
use inferno::collapse::perf::{Folder as PFolder, Options as POptions};
use inferno::collapse::Collapse;
use libflate::gzip::Decoder;
use std::fs::File;
use std::io::{BufReader, Cursor, Read};

fn dtrace_benchmark(c: &mut Criterion) {
    let mut f = File::open("flamegraph/example-dtrace-stacks.txt").expect("file not found");

    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).expect("Culd not read file");

    let len = bytes.len();
    c.bench(
        "collapse",
        ParameterizedBenchmark::new(
            "dtrace",
            move |b, data| {
                b.iter(|| {
                    let reader = BufReader::new(data.as_slice());
                    let mut result = Cursor::new(Vec::with_capacity(len));
                    result.set_position(0);
                    let _folder = DFolder::from(DOptions::default()).collapse(reader, &mut result);
                })
            },
            vec![bytes],
        )
        .throughput(|bytes| Throughput::Bytes(bytes.len() as u32)),
    );
}

fn perf_benchmark(c: &mut Criterion) {
    let f = File::open("flamegraph/example-perf-stacks.txt.gz").expect("file not found");

    let mut bytes = Vec::new();

    let mut r = BufReader::new(Decoder::new(f).unwrap());
    r.read_to_end(&mut bytes).expect("Culd not read file");

    let len = bytes.len();
    c.bench(
        "collapse",
        ParameterizedBenchmark::new(
            "perf",
            move |b, data| {
                b.iter(|| {
                    let reader = BufReader::new(data.as_slice());
                    let mut result = Cursor::new(Vec::with_capacity(len));
                    result.set_position(0);
                    let _folder = PFolder::from(POptions::default()).collapse(reader, &mut result);
                })
            },
            vec![bytes],
        )
        .throughput(|bytes| Throughput::Bytes(bytes.len() as u32)),
    );
}

criterion_group!(benches, perf_benchmark, dtrace_benchmark);
criterion_main!(benches);
