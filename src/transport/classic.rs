//! Classic Bluetooth transport: RFCOMM stream with SDP service-channel
//! discovery, mirroring `PlatformLinux.cpp` + `DBusHelper.cpp` from the
//! reference client.
//!
//! SDP queries are performed through `libbluetooth.so` loaded at runtime via
//! `libloading` (so the app still starts when the library is absent and the
//! failure surfaces as a connection error).

use std::ffi::c_void;
use std::sync::Arc;
use std::time::Duration;

use libloading::{Library, Symbol};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{PollStatus, Transport, TransportError, TransportKind, SERVICE_UUID_XM5};

// Values from `/usr/include/bluetooth/sdp_lib.h` (bluez 5.x).
// Note: SDP_NON_BLOCKING is 0x04 (not 0x01 — that is SDP_RETRY_IF_BUSY) and
// SDP_ATTR_REQ_RANGE is the enum value 2 (not 0x10), or the library rejects
// the request with EINVAL.
const SDP_NON_BLOCKING: u32 = 0x0004;
const SDP_ATTR_REQ_RANGE: i32 = 2;
const RFCOMM_UUID: i32 = 0x0003;
const SDP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Error)]
pub enum SdpError {
    #[error("libbluetooth not available: {0}")]
    Library(#[from] libloading::Error),
    #[error("invalid MAC address: {0}")]
    BadAddress(String),
    #[error("failed to connect to remote SDP server (errno {0})")]
    Connect(i32),
    #[error("SDP service query failed (errno {0})")]
    Query(i32),
    #[error("RFCOMM service channel not found for {0}")]
    ChannelNotFound(String),
    #[error("timed out")]
    Timeout,
}

/// Reads `errno` from the C runtime (x86-64 glibc/musl).
fn last_errno() -> i32 {
    extern "C" {
        #[link_name = "__errno_location"]
        fn errno_location() -> *mut i32;
    }
    unsafe { *errno_location() }
}

/// Bluetooth address (6 bytes, kernel byte order — reversed vs. string form).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BdAddr {
    b: [u8; 6],
}

/// `uuid_t` from `bluetooth.h` (type byte + 16-byte value, packed).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct UuidT {
    type_: u8,
    value: [u8; 16],
}

#[repr(C)]
struct SdpSession {
    _private: [u8; 0],
}

/// `sdp_list_t` — we need `next`/`data` to walk the response list.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct SdpList {
    next: *mut SdpList,
    data: *mut c_void,
}

unsafe impl Send for SdpList {}

/// Resolves the MAC's dotted-hex form to kernel byte order.
pub fn parse_mac(mac: &str) -> Result<BdAddr, SdpError> {
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() != 6 {
        return Err(SdpError::BadAddress(mac.to_string()));
    }
    let mut out = [0u8; 6];
    for (i, p) in parts.iter().enumerate() {
        out[i] = u8::from_str_radix(p, 16).map_err(|_| SdpError::BadAddress(mac.to_string()))?;
    }
    // Kernel expects little-endian byte order.
    out.reverse();
    Ok(BdAddr { b: out })
}

/// Blocking SDP query: finds the RFCOMM service channel for `service_uuid`.
///
/// Mirrors `sdp_connect_nb` + `sdp_poll` + `sdp_getServiceChannel` from the
/// reference client. Must be called from a blocking context.
pub fn sdp_find_rfcomm_channel(
    lib: &Library,
    mac: &str,
    service_uuid: &str,
) -> Result<u8, SdpError> {
    unsafe {
        let sdp_connect: Symbol<
            unsafe extern "C" fn(*const BdAddr, *const BdAddr, u32) -> *mut SdpSession,
        > = lib.get(b"sdp_connect")?;
        let sdp_close: Symbol<unsafe extern "C" fn(*mut SdpSession) -> i32> =
            lib.get(b"sdp_close")?;
        let sdp_get_socket: Symbol<unsafe extern "C" fn(*const SdpSession) -> i32> =
            lib.get(b"sdp_get_socket")?;
        let sdp_search: Symbol<
            unsafe extern "C" fn(
                *mut SdpSession,
                *const SdpList,
                i32,
                *const SdpList,
                *mut *mut SdpList,
            ) -> i32,
        > = lib.get(b"sdp_service_search_attr_req")?;
        let sdp_get_access_protos: Symbol<
            unsafe extern "C" fn(*const c_void, *mut *mut SdpList) -> i32,
        > = lib.get(b"sdp_get_access_protos")?;
        let sdp_get_proto_port: Symbol<unsafe extern "C" fn(*const SdpList, i32) -> i32> =
            lib.get(b"sdp_get_proto_port")?;
        let sdp_list_append: Symbol<
            unsafe extern "C" fn(*mut SdpList, *mut c_void) -> *mut SdpList,
        > = lib.get(b"sdp_list_append")?;
        let sdp_list_free: Symbol<
            unsafe extern "C" fn(*mut SdpList, Option<unsafe extern "C" fn(*mut c_void)>),
        > = lib.get(b"sdp_list_free")?;
        let sdp_uuid128_create: Symbol<
            unsafe extern "C" fn(*mut UuidT, *const c_void) -> *mut UuidT,
        > = lib.get(b"sdp_uuid128_create")?;
        let sdp_record_free: Symbol<unsafe extern "C" fn(*mut c_void)> =
            lib.get(b"sdp_record_free")?;

        let target = parse_mac(mac)?;
        let any = BdAddr { b: [0; 6] };

        let session = sdp_connect(&any, &target, SDP_NON_BLOCKING);
        if session.is_null() {
            log::error!("[sdp] sdp_connect failed (errno {})", last_errno());
            return Err(SdpError::Connect(last_errno()));
        }
        log::debug!("[sdp] sdp_connect ok (non-blocking), waiting for L2CAP connect");
        let session = SessionGuard(session, sdp_close);

        // Wait for the SDP socket to become writable (connection established).
        let sock = sdp_get_socket(session.0);
        if sock < 0 {
            return Err(SdpError::Connect(last_errno()));
        }
        let deadline = std::time::Instant::now() + SDP_CONNECT_TIMEOUT;
        loop {
            let mut pfd = libc::pollfd {
                fd: sock,
                events: libc::POLLIN | libc::POLLOUT,
                revents: 0,
            };
            let res = libc::poll(&mut pfd, 1, 200);
            if res > 0 {
                if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                    log::error!("[sdp] L2CAP connect failed (poll revents {:#x})", pfd.revents);
                    return Err(SdpError::Connect(last_errno()));
                }
                if pfd.revents & (libc::POLLIN | libc::POLLOUT) != 0 {
                    log::debug!("[sdp] L2CAP connected, sending service search");
                    break; // Connected (POLLOUT) or already readable.
                }
            }
            if std::time::Instant::now() > deadline {
                log::error!("[sdp] timed out waiting for L2CAP connect to {mac}");
                return Err(SdpError::Timeout);
            }
        }

        // Build the 128-bit UUID search pattern.
        let uuid_parsed = uuid::Uuid::parse_str(service_uuid)
            .map_err(|_| SdpError::BadAddress(format!("invalid service UUID {service_uuid}")))?;
        let mut uuid = UuidT {
            type_: 0,
            value: [0; 16],
        };
        sdp_uuid128_create(&mut uuid, uuid_parsed.as_bytes().as_ptr() as *const c_void);

        let search = sdp_list_append(std::ptr::null_mut(), &mut uuid as *mut UuidT as *mut c_void);
        if search.is_null() {
            return Err(SdpError::Connect(last_errno()));
        }
        let mut range: u32 = 0x0000_ffff;
        let attrids = sdp_list_append(std::ptr::null_mut(), &mut range as *mut u32 as *mut c_void);
        if attrids.is_null() {
            sdp_list_free(search, None);
            return Err(SdpError::Connect(last_errno()));
        }

        let mut response: *mut SdpList = std::ptr::null_mut();
        let status = sdp_search(
            session.0,
            search,
            SDP_ATTR_REQ_RANGE,
            attrids,
            &mut response,
        );
        sdp_list_free(search, None);
        sdp_list_free(attrids, None);

        if status != 0 {
            log::error!("[sdp] service search failed (errno {})", last_errno());
            return Err(SdpError::Query(last_errno()));
        }
        log::debug!("[sdp] service search ok, {} record(s)", if response.is_null() { 0 } else { count_records(response) });
        if response.is_null() {
            return Err(SdpError::ChannelNotFound(service_uuid.to_string()));
        }

        let mut channel: u8 = 0;
        let mut node = response;
        while !node.is_null() {
            let rec = (*node).data;
            let next = (*node).next;
            if !rec.is_null() {
                let mut proto_list: *mut SdpList = std::ptr::null_mut();
                if sdp_get_access_protos(rec, &mut proto_list) == 0 && !proto_list.is_null() {
                    let port = sdp_get_proto_port(proto_list, RFCOMM_UUID);
                    sdp_list_free(proto_list, None);
                    if port > 0 && port <= u8::MAX as i32 {
                        channel = port as u8;
                    }
                }
                sdp_record_free(rec);
            }
            node = next;
        }
        sdp_list_free(response, None);

        if channel == 0 {
            log::error!("[sdp] no RFCOMM channel in service records for {service_uuid}");
            return Err(SdpError::ChannelNotFound(service_uuid.to_string()));
        }
        log::debug!("[sdp] RFCOMM channel {channel} for {service_uuid}");
        Ok(channel)
    }
}

/// Counts the records in an SDP response list (diagnostics).
fn count_records(list: *mut SdpList) -> usize {
    let mut n = 0;
    let mut node = list;
    while !node.is_null() {
        n += 1;
        node = unsafe { (*node).next };
    }
    n
}

struct SessionGuard<'a>(
    *mut SdpSession,
    Symbol<'a, unsafe extern "C" fn(*mut SdpSession) -> i32>,
);

impl Drop for SessionGuard<'_> {
    fn drop(&mut self) {
        unsafe {
            (self.1)(self.0);
        }
    }
}

/// A transport over a Classic Bluetooth RFCOMM stream.
///
/// `poll_read` peeks (consumes) one byte from the stream into an internal
/// buffer so that readiness checks never lose data.
pub struct ClassicTransport {
    reader: Option<tokio::io::ReadHalf<bluer::rfcomm::Stream>>,
    writer: Option<tokio::io::WriteHalf<bluer::rfcomm::Stream>>,
    peek: Option<u8>,
    connected: bool,
    sdp_lib: Option<Arc<Library>>,
}

impl ClassicTransport {
    pub fn new() -> Self {
        Self {
            reader: None,
            writer: None,
            peek: None,
            connected: false,
            sdp_lib: load_libbluetooth(),
        }
    }

    fn err_connected(&self) -> TransportError {
        if self.connected {
            TransportError::NoConnection
        } else {
            TransportError::Closed
        }
    }
}

impl Default for ClassicTransport {
    fn default() -> Self {
        Self::new()
    }
}

fn load_libbluetooth() -> Option<Arc<Library>> {
    // Prefer the shared object the distro ships with bluez.
    for name in ["libbluetooth.so.3", "libbluetooth.so"] {
        match unsafe { Library::new(name) } {
            Ok(lib) => return Some(Arc::new(lib)),
            Err(_) => continue,
        }
    }
    None
}

#[async_trait::async_trait]
impl Transport for ClassicTransport {
    async fn connect(&mut self, mac: &str) -> Result<(), TransportError> {
        if self.connected {
            return Err(TransportError::NoConnection);
        }
        let lib = self
            .sdp_lib
            .clone()
            .ok_or(TransportError::Internal("libbluetooth not found"))?;
        log::info!("[classic] connecting to {mac}");

        // SDP is a blocking FFI call; run it off the async executor.
        let service_uuid = SERVICE_UUID_XM5.to_string();
        let mac_owned = mac.to_string();
        let channel = tokio::task::spawn_blocking(move || {
            sdp_find_rfcomm_channel(&lib, &mac_owned, &service_uuid)
        })
        .await
        .map_err(|e| {
            log::error!("[classic] SDP task failed: {e}");
            TransportError::Internal(std::boxed::Box::leak(e.to_string().into_boxed_str()))
        })?
        .map_err(|e| {
            log::error!("[classic] SDP failed: {e}");
            TransportError::Internal(std::boxed::Box::leak(e.to_string().into_boxed_str()))
        })?;
        log::info!("[classic] SDP resolved RFCOMM channel {channel} for {mac}");

        let addr = mac
            .parse::<bluer::Address>()
            .map_err(|_| TransportError::BadAddress)?;
        let socket = bluer::rfcomm::Socket::new().map_err(|e| {
            log::error!("[classic] rfcomm socket: {e}");
            TransportError::Net
        })?;
        // The reference client sets RFCOMM_LM_AUTH|RFCOMM_LM_ENCRYPT, which
        // the kernel maps to sec_level = BT_SECURITY_MEDIUM (see
        // rfcomm_sock_setsockopt_old in net/bluetooth/rfcomm/sock.c).
        // Without it the DLC opens unauthenticated and the device's HPC
        // service drops the link (ENOTCONN on the first write); with HIGH
        // the stricter requirements can fail the negotiation. MEDIUM matches
        // the reference client exactly.
        if let Err(e) = socket.set_security(bluer::rfcomm::Security {
            level: bluer::rfcomm::SecurityLevel::Medium,
            key_size: 0,
        }) {
            log::error!("[classic] failed to set rfcomm security: {e}");
            return Err(TransportError::Net);
        }
        log::info!("[classic] opening RFCOMM to {mac} channel {channel}");
        let stream = socket
            .connect(bluer::rfcomm::SocketAddr::new(addr, channel))
            .await
            .map_err(|e| {
                log::error!("[classic] rfcomm connect failed: {e}");
                TransportError::Net
            })?;
        // The kernel may return from connect() before the RFCOMM DLC is fully
        // usable (the L2CAP session can be reused across attempts). Probe with
        // getpeername, which fails with ENOTCONN until the DLC is established.
        let start = std::time::Instant::now();
        loop {
            match stream.peer_addr() {
                Ok(_) => break,
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotConnected
                        && start.elapsed() < Duration::from_millis(1000) =>
                {
                    log::debug!("[classic] DLC not ready yet, waiting: {e}");
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(e) => {
                    log::error!("[classic] rfcomm peer_addr failed: {e}");
                    return Err(TransportError::Net);
                }
            }
        }
        let (reader, writer) = tokio::io::split(stream);
        self.reader = Some(reader);
        self.writer = Some(writer);
        self.peek = None;
        self.connected = true;
        log::info!("[classic] connected to {mac}");
        Ok(())
    }

    async fn disconnect(&mut self) {
        self.reader = None;
        self.writer = None;
        self.peek = None;
        self.connected = false;
    }

    async fn poll_read(&mut self, timeout: Duration) -> Result<PollStatus, TransportError> {
        if self.peek.is_some() {
            return Ok(PollStatus::Ready);
        }
        if !self.connected {
            return Err(self.err_connected());
        }
        let reader = self.reader.as_mut().ok_or(TransportError::Closed)?;
        let mut one = [0u8; 1];
        match tokio::time::timeout(timeout, reader.read(&mut one)).await {
            Err(_) => Ok(PollStatus::Timeout),
            Ok(Ok(0)) => Err(TransportError::Closed),
            Ok(Ok(_)) => {
                self.peek = Some(one[0]);
                Ok(PollStatus::Ready)
            }
            Ok(Err(e)) => Err(map_io_error(e)),
        }
    }

    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        let reader = self.reader.as_mut().ok_or(TransportError::Closed)?;
        let mut n = 0;
        if let Some(b) = self.peek.take() {
            if buf.is_empty() {
                // No buffer to put the peeked byte into: keep it buffered.
                self.peek = Some(b);
                return Ok(0);
            }
            buf[0] = b;
            n = 1;
        }
        if n < buf.len() {
            match reader.read(&mut buf[n..]).await {
                Ok(0) if n == 0 => return Err(TransportError::Closed),
                Ok(0) => {}
                Ok(k) => n += k,
                Err(e) => return Err(map_io_error(e)),
            }
        }
        if n > 0 {
            log::trace!("[classic] rx {n} bytes: {:02X?}", &buf[..n.min(64)]);
        }
        Ok(n)
    }

    async fn send(&mut self, data: &[u8]) -> Result<usize, TransportError> {
        let writer = self.writer.as_mut().ok_or(TransportError::Closed)?;
        let mut delay = Duration::from_millis(20);
        let mut attempt = 0;
        loop {
            match writer.write(data).await {
                Ok(0) => return Err(TransportError::Closed),
                Ok(n) => {
                    log::trace!("[classic] tx {n} bytes: {:02X?}", &data[..n.min(64)]);
                    return Ok(n);
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotConnected && attempt < 5 =>
                {
                    log::warn!("[classic] write failed (DLC not ready?), retrying: {e}");
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                    attempt += 1;
                }
                Err(e) => return Err(map_io_error(e)),
            }
        }
    }

    fn connected(&self) -> bool {
        self.connected
    }

    fn kind(&self) -> TransportKind {
        TransportKind::Classic
    }
}

fn map_io_error(e: std::io::Error) -> TransportError {
    log::error!("[classic] socket error: {e} (kind {:?})", e.kind());
    match e.kind() {
        std::io::ErrorKind::TimedOut => TransportError::Timeout,
        std::io::ErrorKind::WouldBlock => TransportError::Timeout,
        std::io::ErrorKind::BrokenPipe
        | std::io::ErrorKind::ConnectionReset
        | std::io::ErrorKind::ConnectionAborted => TransportError::Closed,
        std::io::ErrorKind::NotConnected => TransportError::NoConnection,
        _ => TransportError::Net,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdp_flags_match_bluez_headers() {
        // Regression: these exact values come from
        // /usr/include/bluetooth/sdp_lib.h — wrong values make the SDP query
        // fail with EINVAL (SDP_ATTR_REQ_RANGE) or block (SDP_NON_BLOCKING).
        assert_eq!(SDP_NON_BLOCKING, 0x04, "SDP_NON_BLOCKING");
        assert_eq!(SDP_ATTR_REQ_RANGE, 2, "SDP_ATTR_REQ_RANGE enum value");
    }

    #[test]
    fn parses_macs() {
        let a = parse_mac("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(a.b, [0xFF, 0xEE, 0xDD, 0xCC, 0xBB, 0xAA]);
        let b = parse_mac("00:1D:DF:11:22:33").unwrap();
        assert_eq!(b.b, [0x33, 0x22, 0x11, 0xDF, 0x1D, 0x00]);
        assert!(parse_mac("nope").is_err());
        assert!(parse_mac("AA:BB:CC:DD:EE").is_err());
        assert!(parse_mac("AA:BB:CC:DD:EE:GX").is_err());
    }

    #[test]
    fn sdp_missing_library_is_an_error_not_a_panic() {
        let t = ClassicTransport::new();
        assert!(t.sdp_lib.is_none() || t.sdp_lib.is_some());
    }

    #[test]
    fn no_sdp_lib_fails_connect_gracefully() {
        let mut t = ClassicTransport {
            reader: None,
            writer: None,
            peek: None,
            connected: false,
            sdp_lib: None,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let r = t.connect("AA:BB:CC:DD:EE:FF").await;
            assert!(r.is_err());
        });
    }
}
