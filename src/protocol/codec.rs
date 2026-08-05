//! Low-level (de)serialization helpers for MDR payload fields.
//!
//! Mirrors `MDRPod`, `MDRPrefixedString` and `MDRPodArray` from
//! `libmdr/include/mdr/Protocol.hpp`.

use thiserror::Error;

/// Serialization error codes (mirrors `MDR_RESULT_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SerError {
    #[error("buffer too small")]
    BufferTooSmall,
    #[error("malformed payload")]
    Malformed,
    #[error("invalid argument")]
    InvalidArgument,
    #[error("validation failed: {0}")]
    Validation(&'static str),
}

impl SerError {
    pub const fn to_result_code(self) -> i32 {
        match self {
            SerError::BufferTooSmall => 9,
            SerError::Malformed => 10,
            SerError::InvalidArgument => 11,
            SerError::Validation(_) => 10,
        }
    }
}

pub type SerResult<T> = Result<T, SerError>;

/// A cursor over a byte buffer with bounds-checked reads.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub fn take(&mut self, n: usize) -> SerResult<&'a [u8]> {
        if self.remaining() < n {
            return Err(SerError::Malformed);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn take_rest(&mut self) -> &'a [u8] {
        let s = &self.buf[self.pos..];
        self.pos = self.buf.len();
        s
    }

    pub fn u8(&mut self) -> SerResult<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn u16_be(&mut self) -> SerResult<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    pub fn u32_be(&mut self) -> SerResult<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Reads a fixed-size little-endian integer type (used by Int32BE on the wire
    /// the values are stored big-endian; see `Int32BE` in Protocol.hpp).
    pub fn i32_be(&mut self) -> SerResult<i32> {
        Ok(self.u32_be()? as i32)
    }

    pub fn skip(&mut self, n: usize) -> SerResult<()> {
        self.take(n).map(|_| ())
    }
}

/// A growable writer with bounds-checked writes.
#[derive(Debug, Clone, Default)]
pub struct Writer {
    buf: Vec<u8>,
    limit: usize,
}

impl Writer {
    pub fn new(limit: usize) -> Self {
        Self {
            buf: Vec::with_capacity(limit.min(64)),
            limit,
        }
    }

    pub fn with_capacity(cap: usize, limit: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap.min(limit)),
            limit,
        }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.buf
    }

    pub fn u8(&mut self, v: u8) -> SerResult<()> {
        if self.buf.len() + 1 > self.limit {
            return Err(SerError::BufferTooSmall);
        }
        self.buf.push(v);
        Ok(())
    }

    pub fn u16_be(&mut self, v: u16) -> SerResult<()> {
        self.u8((v >> 8) as u8)?;
        self.u8(v as u8)
    }

    pub fn u32_be(&mut self, v: u32) -> SerResult<()> {
        self.u8((v >> 24) as u8)?;
        self.u8((v >> 16) as u8)?;
        self.u8((v >> 8) as u8)?;
        self.u8(v as u8)
    }

    pub fn i32_be(&mut self, v: i32) -> SerResult<()> {
        self.u32_be(v as u32)
    }

    pub fn bytes(&mut self, v: &[u8]) -> SerResult<()> {
        if self.buf.len() + v.len() > self.limit {
            return Err(SerError::BufferTooSmall);
        }
        self.buf.extend_from_slice(v);
        Ok(())
    }

    pub fn prefixed_string(&mut self, v: &str) -> SerResult<()> {
        let b = v.as_bytes();
        if b.len() >= 256 {
            return Err(SerError::InvalidArgument);
        }
        self.u8(b.len() as u8)?;
        self.bytes(b)
    }

    pub fn pod_array<T: Copy>(&mut self, v: &[T]) -> SerResult<()> {
        if v.len() >= 256 {
            return Err(SerError::InvalidArgument);
        }
        self.u8(v.len() as u8)?;
        let bytes = unsafe {
            // Safety: T: Copy, no padding concerns for our enums (repr(u8)) / u8.
            std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v))
        };
        self.bytes(bytes)
    }

    pub fn into_packet_payload(self) -> Vec<u8> {
        self.buf
    }
}

/// Reads a length-prefixed string (1-byte length prefix).
pub fn read_prefixed_string(r: &mut Reader<'_>) -> SerResult<String> {
    let len = r.u8()? as usize;
    let b = r.take(len)?;
    Ok(String::from_utf8_lossy(b).into_owned())
}

/// Reads a 1-byte-count-prefixed array of `repr(u8)` enums / u8 values.
pub fn read_pod_array<T: From<u8> + Copy>(r: &mut Reader<'_>) -> SerResult<Vec<T>> {
    let count = r.u8()? as usize;
    let b = r.take(count * size_of::<T>())?;
    Ok(b.iter().map(|&x| T::from(x)).collect())
}

/// Reads `n` raw bytes as `repr(u8)` enums.
pub fn read_enums<T: From<u8> + Copy>(r: &mut Reader<'_>, n: usize) -> SerResult<Vec<T>> {
    let b = r.take(n)?;
    Ok(b.iter().map(|&x| T::from(x)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_bounds() {
        let mut r = Reader::new(&[1, 2, 3]);
        assert_eq!(r.u8().unwrap(), 1);
        assert_eq!(r.u16_be().unwrap(), 0x0203);
        assert!(r.take(1).is_err());
        assert!(r.u8().is_err());
    }

    #[test]
    fn prefixed_string_roundtrip() {
        let mut w = Writer::new(64);
        w.prefixed_string("WH-1000XM5").unwrap();
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(read_prefixed_string(&mut r).unwrap(), "WH-1000XM5");
    }

    #[test]
    fn prefixed_string_too_long() {
        let mut w = Writer::new(1024);
        let long = "x".repeat(300);
        assert!(w.prefixed_string(&long).is_err());
    }

    #[test]
    fn pod_array_roundtrip() {
        let mut w = Writer::new(64);
        w.pod_array(&[0x01u8, 0x02, 0xFF]).unwrap();
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(read_pod_array::<u8>(&mut r).unwrap(), vec![1, 2, 255]);
    }

    #[test]
    fn writer_limit_enforced() {
        let mut w = Writer::new(2);
        assert!(w.bytes(&[1, 2, 3]).is_err());
        assert!(w.bytes(&[1, 2]).is_ok());
        assert!(w.u8(3).is_err());
    }
}
