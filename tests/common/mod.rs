#![allow(dead_code)]

mod collapse;
pub mod test_logger;

pub use self::collapse::{compare_results, test_collapse, test_collapse_logs};
