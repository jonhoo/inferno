#![no_main]

use inferno::collapse::{Collapse, sample::Folder};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    Folder::default().collapse(data, std::io::sink()).ok();
});
