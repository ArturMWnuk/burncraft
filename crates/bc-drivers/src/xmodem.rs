//! XMODEM-CRC (wysyłka pliku na kartę SD, odmiana FluidNC; ADR 0010 §3).

/// CRC-16/XMODEM (wielomian 0x1021, start 0).
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Pakiety 133 B (SOH, numer, dopełnienie, 128 B danych z dopełnieniem 0x1A, CRC).
pub fn packets(data: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for (i, chunk) in data.chunks(128).enumerate() {
        let n = (i + 1) as u8;
        let mut p = vec![0x01, n, !n];
        let mut d = chunk.to_vec();
        d.resize(128, 0x1A);
        p.extend_from_slice(&d);
        p.extend_from_slice(&crc16(&d).to_be_bytes());
        out.push(p);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn crc16_known_value() {
        assert_eq!(super::crc16(b"123456789"), 0x31C3);
    }
}
