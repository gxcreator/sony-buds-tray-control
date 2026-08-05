//! MDR packet framing: escaping, checksum and pack/unpack.
//!
//! Wire format (mirrors `libmdr/src/Command.cpp`):
//! ```text
//! <START '>'> ESCAPED(<TYPE><SEQ><SIZE BE32><DATA><CHECKSUM>) <END '<'>
//! ```
//! Bytes `0x3C '<'`, `0x3D '='`, `0x3E '>'` inside the escaped payload are
//! escaped as `= ,`, `= -`, `= .` respectively. The checksum is the 8-bit sum
//! of the unescaped bytes (type .. data, excluding the checksum byte itself).

use super::enums::DataType;

pub const ESCAPE_SENTRY: u8 = 0x3D; // '='
pub const ESCAPED_3C: u8 = 44; // '<' -> '=' ','
pub const ESCAPED_3D: u8 = 45; // '=' -> '=' '-'
pub const ESCAPED_3E: u8 = 46; // '>' -> '=' '.'
pub const START_MARKER: u8 = 62; // '>'
pub const END_MARKER: u8 = 60; // '<'

pub const MAX_PACKET_SIZE: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnpackResult {
    Ok,
    Incomplete,
    BadMarker,
    BadChecksum,
    Malformed,
}

/// Packs a serialized command payload into a full MDR packet.
pub fn pack(type_: DataType, seq: u8, data: &[u8]) -> Vec<u8> {
    debug_assert!(data.len() <= u32::MAX as usize, "payload too large");

    let mut unescaped = Vec::with_capacity(data.len() + 7);
    unescaped.push(type_ as u8);
    unescaped.push(seq);
    unescaped.extend_from_slice(&(data.len() as u32).to_be_bytes());
    unescaped.extend_from_slice(data);
    let checksum = unescaped.iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
    unescaped.push(checksum);

    let mut out = Vec::with_capacity(unescaped.len() + 2);
    out.push(START_MARKER);
    out.extend(escape(&unescaped));
    out.push(END_MARKER);
    out
}

/// Unpacks a complete MDR packet (including markers).
pub fn unpack(packed: &[u8]) -> UnpackResult {
    match unpack_full(packed) {
        Ok((_, _, _)) => UnpackResult::Ok,
        Err(e) => e,
    }
}

/// Unpacks a complete MDR packet, returning (type, seq, payload) on success.
pub fn unpack_full(packed: &[u8]) -> Result<(DataType, u8, Vec<u8>), UnpackResult> {
    if packed.len() < 2 {
        return Err(UnpackResult::Incomplete);
    }
    if packed[0] != START_MARKER {
        return Err(UnpackResult::BadMarker);
    }
    if packed[packed.len() - 1] != END_MARKER {
        // Truncated frame: more bytes may still arrive.
        return Err(UnpackResult::Incomplete);
    }
    let unescaped = unescape(&packed[1..packed.len() - 1]).ok_or(UnpackResult::Malformed)?;
    if unescaped.len() < 7 {
        return Err(UnpackResult::Malformed);
    }

    let type_ = DataType::from_u8(unescaped[0]);
    let seq = unescaped[1];
    let size =
        u32::from_be_bytes([unescaped[2], unescaped[3], unescaped[4], unescaped[5]]) as usize;

    // Checksum covers type, seq, size and data (everything except the last byte).
    let checksum = *unescaped.last().unwrap();
    let computed = unescaped[..unescaped.len() - 1]
        .iter()
        .fold(0u8, |acc, b| acc.wrapping_add(*b));
    if checksum != computed {
        return Err(UnpackResult::BadChecksum);
    }

    let data = &unescaped[6..unescaped.len() - 1];
    if data.len() != size {
        // Frame claims a different size than the escaped payload provides.
        // This cannot be fixed by receiving more bytes: drop the frame.
        return Err(UnpackResult::Malformed);
    }
    Ok((type_, seq, data.to_vec()))
}

/// Escapes marker bytes. Never fails; output length is at most 2x input.
pub fn escape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for &b in data {
        match b {
            60 => out.extend_from_slice(&[ESCAPE_SENTRY, ESCAPED_3C]),
            61 => out.extend_from_slice(&[ESCAPE_SENTRY, ESCAPED_3D]),
            62 => out.extend_from_slice(&[ESCAPE_SENTRY, ESCAPED_3E]),
            _ => out.push(b),
        }
    }
    out
}

/// Unescapes marker bytes. Returns `None` on a dangling/unknown escape sequence.
pub fn unescape(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len());
    let mut it = data.iter().copied();
    while let Some(b) = it.next() {
        if b == ESCAPE_SENTRY {
            match it.next()? {
                ESCAPED_3C => out.push(60),
                ESCAPED_3D => out.push(61),
                ESCAPED_3E => out.push(62),
                _ => return None,
            }
        } else {
            out.push(b);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(type_: DataType, seq: u8, data: &[u8]) {
        let packed = pack(type_, seq, data);
        let (t, s, d) = unpack_full(&packed).expect("unpack ok");
        assert_eq!(t, type_);
        assert_eq!(s, seq);
        assert_eq!(d, data);
    }

    #[test]
    fn roundtrip_empty() {
        roundtrip(DataType::Ack, 1, &[]);
    }

    #[test]
    fn roundtrip_plain() {
        roundtrip(DataType::DataMdr, 0, &[0x00, 0x01, 0x02, 0xFF, 0xAB]);
    }

    #[test]
    fn roundtrip_with_marker_bytes() {
        // Data containing '<', '=', '>' must survive the escape round trip.
        roundtrip(DataType::DataMdr, 1, &[0x3C, 0x3D, 0x3E, 0x3C]);
        roundtrip(DataType::DataMdrNo2, 0, &[0x3C]);
    }

    #[test]
    fn checksum_is_sum_of_unescaped_prefix() {
        // Golden: build a packet by hand and verify the checksum byte.
        let data = [0x12u8, 0x02];
        let mut expected = vec![DataType::DataMdr as u8, 0x00];
        expected.extend_from_slice(&(data.len() as u32).to_be_bytes());
        expected.extend_from_slice(&data);
        let sum = expected.iter().fold(0u8, |a, b| a.wrapping_add(*b));
        expected.push(sum);
        let packed = pack(DataType::DataMdr, 0, &data);
        assert_eq!(unescape(&packed[1..packed.len() - 1]), Some(expected));
    }

    #[test]
    fn tampered_checksum_rejected() {
        let mut packed = pack(DataType::DataMdr, 0, &[1, 2, 3]);
        let last = packed.len() - 2; // data byte
        packed[last] ^= 0xFF;
        assert_eq!(unpack(&packed), UnpackResult::BadChecksum);
    }

    #[test]
    fn bad_markers_rejected() {
        let mut packed = pack(DataType::DataMdr, 0, &[1, 2]);
        packed[0] = 0x42;
        assert_eq!(unpack(&packed), UnpackResult::BadMarker);
        let mut packed = pack(DataType::DataMdr, 0, &[1, 2]);
        *packed.last_mut().unwrap() = 0x42;
        // Missing end marker: more bytes may still arrive.
        assert_eq!(unpack(&packed), UnpackResult::Incomplete);
    }

    #[test]
    fn malformed_size_rejected() {
        // Frame whose declared size doesn't match its payload length.
        let mut packed = pack(DataType::DataMdr, 0, &[1, 2, 3]);
        packed[2 + 2] = 0x7F; // bump the size field, checksum now invalid but size check is first
                              // Recompute checksum so we specifically exercise the size-mismatch path.
        let body = packed.clone();
        // find the '<' end marker
        let end = body.len() - 1;
        let mut unescaped = unescape(&packed[1..end]).unwrap();
        let n = unescaped.len();
        unescaped[n - 1] = unescaped[..n - 1]
            .iter()
            .fold(0u8, |a, b| a.wrapping_add(*b));
        let mut rebuilt = vec![START_MARKER];
        rebuilt.extend(escape(&unescaped));
        rebuilt.push(END_MARKER);
        assert_eq!(unpack(&rebuilt), UnpackResult::Malformed);
    }

    #[test]
    fn truncated_frames_are_incomplete() {
        let packed = pack(DataType::DataMdr, 0, &[1, 2, 3]);
        assert_eq!(
            unpack(&packed[..packed.len() - 1]),
            UnpackResult::Incomplete
        );
        assert_eq!(unpack(&[]), UnpackResult::Incomplete);
    }

    #[test]
    fn dangling_escape_is_malformed() {
        assert_eq!(unescape(&[0x3D]), None);
        assert_eq!(unescape(&[0x3D, 0x2A]), None);
    }

    #[test]
    fn escape_expands_markers_only() {
        assert_eq!(escape(&[0x3C]), vec![0x3D, 44]);
        assert_eq!(escape(&[0x3D]), vec![0x3D, 45]);
        assert_eq!(escape(&[0x3E]), vec![0x3D, 46]);
        assert_eq!(escape(&[0x41]), vec![0x41]);
    }
}
