//! REQ-19-003: importer SVG nie może spanikować ani zawiesić się na dowolnych danych.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = bc_io::svg::import(data, &bc_io::ImportOptions::default());
});
