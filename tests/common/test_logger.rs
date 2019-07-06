// Before this crate's collapsers were capable of working across multiple threads, we were using
// the `testing_logger` crate (https://crates.io/crates/testing_logger) to help test log messages.
// This crate used a thread-local variable to store captured logs for inspection. As our logging
// started occurring not just on the main thread, but also on worker threads, we could not longer
// properly test our logs with this crate. The code below is similar to the `testing_logger` crate,
// but stores captured logs in a global variable protected by a mutex instead of in a thread-local
// variable; so it works in a multi-threaded environment. The only caveat is, to work properly, we
// must run tests with the `test-threads` flag set to 1 (i.e. `cargo test -- --test-threads=1`) so
// that one test does not interfere with another test running at the same time. In the future, we
// may swap out our logging implementation for a more robust one, in which case the `test-threads=1`
// limitation would no longer be necessary, but that work is not yet complete.

use std::ops::Deref;
use std::sync::{Arc, Mutex, Once};

use lazy_static::lazy_static;
use log::{Level, LevelFilter, Log, Metadata, Record};

static INIT: Once = Once::new();
static TEST_LOGGER: TestLogger = TestLogger;

lazy_static! {
    static ref CAPTURED_LOGS: Arc<Mutex<Vec<CapturedLog>>> = Arc::new(Mutex::new(Vec::new()));
}

pub fn init() {
    INIT.call_once(|| {
        log::set_logger(&TEST_LOGGER).unwrap();
        log::set_max_level(LevelFilter::Trace);
    });
    let mut guard = match CAPTURED_LOGS.lock() {
        Ok(guard) => guard,
        Err(e) => e.into_inner(),
    };
    guard.clear();
}

pub fn validate<F>(asserter: F)
where
    F: Fn(&Vec<CapturedLog>),
{
    let mut guard = match CAPTURED_LOGS.lock() {
        Ok(guard) => guard,
        Err(e) => e.into_inner(),
    };
    asserter(guard.deref());
    guard.clear();
}

#[derive(Debug)]
pub struct CapturedLog {
    pub body: String,
    pub level: Level,
    pub target: String,
}

struct TestLogger;

impl Log for TestLogger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let captured_log = CapturedLog {
            body: format!("{}", record.args()),
            level: record.level(),
            target: record.target().to_string(),
        };
        let mut guard = match CAPTURED_LOGS.lock() {
            Ok(guard) => guard,
            Err(e) => e.into_inner(),
        };
        guard.push(captured_log);
    }

    fn flush(&self) {}
}
