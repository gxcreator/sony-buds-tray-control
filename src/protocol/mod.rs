//! Sony MDR V2 protocol: enums, packet framing and payload codecs.
//!
//! A faithful, byte-compatible reimplementation of the wire protocol spoken by
//! the `libmdr` library from the SonyHeadphonesClient project.

pub mod codec;
pub mod enums;
pub mod packet;
pub mod payloads;

pub use enums::*;
pub use packet::{
    pack, unpack, unpack_full, UnpackResult, END_MARKER, MAX_PACKET_SIZE, START_MARKER,
};
pub use payloads::*;
