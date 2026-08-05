//! Transport abstraction over the headphone connection.
//!
//! The device engine talks to a [`Transport`]; concrete implementations cover
//! Classic Bluetooth (RFCOMM, `classic.rs`) and BLE (GATT, `gatt.rs`).
//! Tests use the in-memory [`MockTransport`].

pub mod classic;
pub mod discovery;
pub mod gatt;

use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollStatus {
    Ready,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TransportError {
    #[error("resource not found")]
    NotFound,
    #[error("no connection")]
    NoConnection,
    #[error("networking error")]
    Net,
    #[error("invalid address")]
    BadAddress,
    #[error("not supported")]
    NotSupported,
    #[error("timed out")]
    Timeout,
    #[error("connection closed")]
    Closed,
    #[error("internal error: {0}")]
    Internal(&'static str),
}

impl TransportError {
    /// Maps to the `MDR_RESULT_*` codes used by the reference client.
    pub const fn to_mdr_code(self) -> i32 {
        match self {
            TransportError::NotFound => 3,
            TransportError::NoConnection => 6,
            TransportError::Net => 5,
            TransportError::BadAddress => 7,
            TransportError::NotSupported => 8,
            TransportError::Timeout => 4,
            TransportError::Closed | TransportError::Internal(_) => 5,
        }
    }
}

/// A bidirectional byte stream to a headphone.
#[async_trait::async_trait]
pub trait Transport: Send {
    /// Establishes the connection to the device identified by `mac`.
    async fn connect(&mut self, mac: &str) -> Result<(), TransportError>;
    /// Tears the connection down.
    async fn disconnect(&mut self);
    /// Blocks until data is available for reading, or `timeout` elapses.
    async fn poll_read(&mut self, timeout: Duration) -> Result<PollStatus, TransportError>;
    /// Reads at least 1 byte if available; `Ok(0)` signals EOF.
    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize, TransportError>;
    /// Sends as many bytes as possible; returns the number actually sent.
    async fn send(&mut self, data: &[u8]) -> Result<usize, TransportError>;
    fn connected(&self) -> bool;
    fn kind(&self) -> TransportKind;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    Classic,
    Ble,
}

impl TransportKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            TransportKind::Classic => "Classic",
            TransportKind::Ble => "BLE",
        }
    }
}

/// Delegating implementation so `Box<dyn Transport>` can drive the engine.
#[async_trait::async_trait]
impl Transport for Box<dyn Transport> {
    async fn connect(&mut self, mac: &str) -> Result<(), TransportError> {
        (**self).connect(mac).await
    }

    async fn disconnect(&mut self) {
        (**self).disconnect().await
    }

    async fn poll_read(&mut self, timeout: Duration) -> Result<PollStatus, TransportError> {
        (**self).poll_read(timeout).await
    }

    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        (**self).recv(buf).await
    }

    async fn send(&mut self, data: &[u8]) -> Result<usize, TransportError> {
        (**self).send(data).await
    }

    fn connected(&self) -> bool {
        (**self).connected()
    }

    fn kind(&self) -> TransportKind {
        (**self).kind()
    }
}

/// Service UUIDs from `mdr-c/Base.h`.
pub const SERVICE_UUID_XM5: &str = "956C7B26-D49A-4BA8-B03F-B17D393CB6E2";
pub const BLE_SERVICE_UUID_TANDEM_OVER_BLE_HPC: &str = "5B833E20-6BC7-4802-8E9A-723CECA4BD8F";

// ---------------------------------------------------------------------------
// Mock transport
// ---------------------------------------------------------------------------

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

/// One direction of an in-memory byte pipe (used by the mock transport and
/// by the GATT transport's notification bridge).
#[derive(Debug, Default)]
pub(crate) struct Pipe {
    buf: Mutex<VecDeque<u8>>,
    notify: Notify,
    closed: AtomicBool,
}

impl Pipe {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            buf: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
            closed: AtomicBool::new(false),
        })
    }

    pub(crate) fn push(&self, data: &[u8]) -> bool {
        if self.closed.load(Ordering::SeqCst) {
            return false;
        }
        self.buf.lock().unwrap().extend(data.iter().copied());
        self.notify.notify_waiters();
        true
    }

    pub(crate) fn pop(&self, dst: &mut [u8]) -> Option<usize> {
        if dst.is_empty() {
            return Some(0);
        }
        let mut buf = self.buf.lock().unwrap();
        if buf.is_empty() {
            return None;
        }
        let n = buf.len().min(dst.len());
        for (i, b) in buf.drain(..n).enumerate() {
            dst[i] = b;
        }
        Some(n)
    }

    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }
}

/// In-memory transport used by tests; also the host side of the [`MockDevice`]
/// test double (see `tests/common/mod.rs`).
#[derive(Debug)]
pub struct MockTransport {
    rx: Arc<Pipe>,
    tx: Arc<Pipe>,
    connected: bool,
}

impl MockTransport {
    /// Creates a connected transport pair: `(host, device)`.
    pub fn pair() -> (MockTransport, MockTransport) {
        let a2b = Pipe::new();
        let b2a = Pipe::new();
        (
            MockTransport {
                rx: b2a.clone(),
                tx: a2b.clone(),
                connected: false,
            },
            MockTransport {
                rx: a2b,
                tx: b2a,
                connected: false,
            },
        )
    }

    /// Bytes available for reading right now (test helper).
    pub fn pending(&self) -> usize {
        self.rx.buf.lock().unwrap().len()
    }
}

#[async_trait::async_trait]
impl Transport for MockTransport {
    async fn connect(&mut self, _mac: &str) -> Result<(), TransportError> {
        self.connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) {
        self.connected = false;
        self.rx.close();
        self.tx.close();
    }

    async fn poll_read(&mut self, timeout: Duration) -> Result<PollStatus, TransportError> {
        if self.rx.closed.load(Ordering::SeqCst) {
            return Err(TransportError::Closed);
        }
        if !self.rx.buf.lock().unwrap().is_empty() {
            return Ok(PollStatus::Ready);
        }
        match tokio::time::timeout(timeout, self.rx.notify.notified()).await {
            Ok(_) => Ok(PollStatus::Ready),
            Err(_) => Ok(PollStatus::Timeout),
        }
    }

    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        loop {
            if let Some(n) = self.rx.pop(buf) {
                return Ok(n);
            }
            if self.rx.closed.load(Ordering::SeqCst) {
                return Ok(0);
            }
            self.rx.notify.notified().await;
        }
    }

    async fn send(&mut self, data: &[u8]) -> Result<usize, TransportError> {
        if !self.tx.push(data) {
            return Err(TransportError::Closed);
        }
        Ok(data.len())
    }

    fn connected(&self) -> bool {
        self.connected
    }

    fn kind(&self) -> TransportKind {
        TransportKind::Classic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_pipe_roundtrip() {
        let (mut a, mut b) = MockTransport::pair();
        a.connect("AA:BB:CC:DD:EE:FF").await.unwrap();
        b.connect("AA:BB:CC:DD:EE:FF").await.unwrap();

        assert_eq!(a.send(&[1, 2, 3]).await.unwrap(), 3);
        assert_eq!(
            b.poll_read(Duration::from_millis(100)).await.unwrap(),
            PollStatus::Ready
        );
        let mut buf = [0u8; 8];
        assert_eq!(b.recv(&mut buf).await.unwrap(), 3);
        assert_eq!(&buf[..3], &[1, 2, 3]);

        assert_eq!(
            b.poll_read(Duration::from_millis(10)).await,
            Ok(PollStatus::Timeout)
        );
    }

    #[tokio::test]
    async fn mock_pipe_partial_reads() {
        let (mut a, mut b) = MockTransport::pair();
        a.connect("").await.unwrap();
        b.connect("").await.unwrap();
        a.send(&[1, 2, 3, 4, 5]).await.unwrap();
        let mut buf = [0u8; 2];
        assert_eq!(b.recv(&mut buf).await.unwrap(), 2);
        assert_eq!(&buf, &[1, 2]);
    }

    #[tokio::test]
    async fn mock_pipe_eof_on_close() {
        let (mut a, mut b) = MockTransport::pair();
        b.connect("").await.unwrap();
        a.connect("").await.unwrap();
        a.disconnect().await;
        let mut buf = [0u8; 4];
        assert_eq!(b.recv(&mut buf).await.unwrap(), 0);
        assert!(matches!(
            b.poll_read(Duration::from_millis(100)).await,
            Err(TransportError::Closed)
        ));
    }
}
