//! REQ-19-004: importer DXF nie może spanikować na dowolnych danych.
//!
//! Panika wewnątrz biblioteki `dxf` jest przechwytywana przez importer
//! (`catch_unwind` → błąd parsowania), więc wyciszamy tylko hak paniki;
//! panika, która wydostanie się z importera, nadal kończy się awarią.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(
    init: {
        std::panic::set_hook(Box::new(|_| {}));
    },
    |data: &[u8]| {
        let _ = bc_io::dxf_io::import(data, &bc_io::ImportOptions::default());
    }
);
