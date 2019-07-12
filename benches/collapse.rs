use std::fs::File;
use std::io::{self, BufReader, Read};

use criterion::*;
use inferno::collapse::{dtrace, perf, sample, Collapse};
use libflate::gzip::Decoder;

const INFILE_DTRACE: &str = "flamegraph/example-dtrace-stacks.txt";
const INFILE_PERF: &str = "flamegraph/example-perf-stacks.txt.gz";
const INFILE_SAMPLE: &str = "tests/data/collapse-sample/large.txt.gz";

fn collapse_benchmark<C>(c: &mut Criterion, mut collapser: C, id: &str, infile: &str)
where
    C: 'static + Collapse + Clone,
{
    let mut f = File::open(infile).expect("file not found");

    let mut bytes = Vec::new();
    if infile.ends_with(".gz") {
        let mut r = BufReader::new(Decoder::new(f).unwrap());
        r.read_to_end(&mut bytes).expect("Could not read file");
    } else {
        f.read_to_end(&mut bytes).expect("Could not read file");
    }

    collapser.set_nthreads(1);

    let nthreads = num_cpus::get();
    let mut collapser2 = collapser.clone();
    collapser2.set_nthreads(nthreads);

    c.bench(
        "collapse",
        ParameterizedBenchmark::new(
            format!("{}/1", id),
            move |b, data| {
                b.iter(|| {
                    let _result = collapser.collapse(data.as_slice(), io::sink());
                })
            },
            vec![bytes],
        )
        .with_function(format!("{}/{}", id, nthreads), move |b, data| {
            b.iter(|| {
                let _result = collapser2.collapse(data.as_slice(), io::sink());
            })
        })
        .throughput(|bytes| Throughput::Bytes(bytes.len() as u32))
        .sample_size(100),
    );
}

fn dtrace(c: &mut Criterion) {
    collapse_benchmark(c, dtrace::Folder::default(), "dtrace", INFILE_DTRACE);
}

fn perf(c: &mut Criterion) {
    collapse_benchmark(c, perf::Folder::default(), "perf", INFILE_PERF);
}

fn sample(c: &mut Criterion) {
    collapse_benchmark(c, sample::Folder::default(), "sample", INFILE_SAMPLE);
}

criterion_group!(benches, dtrace, perf, sample,);

criterion_main!(benches);
