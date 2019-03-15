extern crate criterion;
extern crate inferno;

use criterion::*;
use inferno::collapse::dtrace;
use inferno::collapse::perf;
use inferno::collapse::Collapse;
use libflate::gzip::Decoder;
use std::fs::File;
use std::io::{self, BufReader, Read};

fn collapse_benchmark<C>(c: &mut Criterion, mut collapser: C, id: &str, infile: &str)
where
    C: 'static + Collapse,
{
    let mut f = File::open(infile).expect("file not found");

    let mut bytes = Vec::new();
    if infile.ends_with(".gz") {
        let mut r = BufReader::new(Decoder::new(f).unwrap());
        r.read_to_end(&mut bytes).expect("Could not read file");
    } else {
        f.read_to_end(&mut bytes).expect("Could not read file");
    }

    c.bench(
        "collapse",
        ParameterizedBenchmark::new(
            id,
            move |b, data| {
                b.iter(|| {
                    let reader = BufReader::new(data.as_slice());
                    let _folder = collapser.collapse(reader, io::sink());
                })
            },
            vec![bytes],
        )
        .throughput(|bytes| Throughput::Bytes(bytes.len() as u32)),
    );
}

fn dtrace(c: &mut Criterion) {
    let infile = "flamegraph/example-dtrace-stacks.txt";
    collapse_benchmark(
        c,
        dtrace::Folder::from(dtrace::Options::default()),
        "dtrace",
        infile,
    );
}

fn perf(c: &mut Criterion) {
    let infile = "flamegraph/example-perf-stacks.txt.gz";
    collapse_benchmark(
        c,
        perf::Folder::from(perf::Options::default()),
        "perf",
        infile,
    );
}

criterion_group!(benches, dtrace, perf);
criterion_main!(benches);
