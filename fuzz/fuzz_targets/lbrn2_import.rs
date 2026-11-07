//! REQ-19-005: importer .lbrn2 nie może spanikować na dowolnych danych.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = bc_io::lbrn::import_lbrn2(data, &bc_io::ImportOptions::default());
});
