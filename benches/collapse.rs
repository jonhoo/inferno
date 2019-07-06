use std::fs::File;
use std::io::{self, BufReader, Read};

use criterion::*;
use inferno::collapse::{dtrace, perf, Collapse};
use libflate::gzip::Decoder;

const INFILE_DTRACE: &str = "flamegraph/example-dtrace-stacks.txt";
const INFILE_PERF: &str = "flamegraph/example-perf-stacks.txt.gz";

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
                    let _folder = collapser.collapse(data.as_slice(), io::sink());
                })
            },
            vec![bytes],
        )
        .sample_size(100)
        .throughput(|bytes| Throughput::Bytes(bytes.len() as u32)),
    );
}

fn dtrace_single(c: &mut Criterion) {
    let mut options = dtrace::Options::default();
    options.nthreads = 1;
    collapse_benchmark(
        c,
        dtrace::Folder::from(options),
        "dtrace_single",
        INFILE_DTRACE,
    );
}

fn dtrace_multi(c: &mut Criterion) {
    let mut options = dtrace::Options::default();
    options.nthreads = num_cpus::get();
    collapse_benchmark(
        c,
        dtrace::Folder::from(options),
        "dtrace_multi",
        INFILE_DTRACE,
    );
}

fn perf_single(c: &mut Criterion) {
    let mut options = perf::Options::default();
    options.nthreads = 1;
    collapse_benchmark(c, perf::Folder::from(options), "perf_single", INFILE_PERF);
}

fn perf_multi(c: &mut Criterion) {
    let mut options = perf::Options::default();
    options.nthreads = num_cpus::get();
    collapse_benchmark(c, perf::Folder::from(options), "perf_multi", INFILE_PERF);
}

criterion_group!(
    benches,
    dtrace_single,
    dtrace_multi,
    perf_single,
    perf_multi
);
criterion_main!(benches);
