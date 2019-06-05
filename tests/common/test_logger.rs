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
