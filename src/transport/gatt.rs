//! BLE (GATT) transport over BlueZ D-Bus, mirroring the Windows BLE backend
//! of the reference client: locate the target service, pick the first
//! writable and the first notifiable characteristic, subscribe to
//! notifications and bridge them into an in-memory pipe.

use std::time::Duration;

use bluer::gatt::remote::Characteristic;
use std::sync::Arc;

use futures::StreamExt;
use tokio::task::JoinHandle;

use super::{
    Pipe, PollStatus, Transport, TransportError, TransportKind,
    BLE_SERVICE_UUID_TANDEM_OVER_BLE_HPC,
};

pub struct GattTransport {
    session: bluer::Session,
    service_uuid: uuid::Uuid,
    write_char: Option<Characteristic>,
    rx: Arc<Pipe>,
    notify_task: Option<JoinHandle<()>>,
    connected: bool,
}

impl GattTransport {
    pub fn new(session: bluer::Session, service_uuid: &str) -> Self {
        let service_uuid = uuid::Uuid::parse_str(service_uuid).unwrap_or_else(|_| {
            uuid::Uuid::parse_str(BLE_SERVICE_UUID_TANDEM_OVER_BLE_HPC).unwrap()
        });
        Self {
            session,
            service_uuid,
            write_char: None,
            rx: Pipe::new(),
            notify_task: None,
            connected: false,
        }
    }
}

#[async_trait::async_trait]
impl Transport for GattTransport {
    async fn connect(&mut self, mac: &str) -> Result<(), TransportError> {
        if self.connected {
            return Err(TransportError::NoConnection);
        }
        log::info!("[gatt] connecting to {mac} (service {})", self.service_uuid);
        let adapter = self.session.default_adapter().await.map_err(|e| {
            log::error!("[gatt] default adapter: {e}");
            TransportError::NotFound
        })?;
        log::debug!("[gatt] adapter: {}", adapter.name());
        let addr: bluer::Address = mac.parse().map_err(|_| TransportError::BadAddress)?;
        let device = adapter.device(addr).map_err(|e| {
            log::error!("[gatt] device lookup: {e}");
            TransportError::NotFound
        })?;

        device.connect().await.map_err(|e| {
            log::error!("[gatt] device connect: {e}");
            TransportError::Net
        })?;
        log::debug!("[gatt] device connected");

        // The service list is only populated once the device is connected.
        let services = device.services().await.map_err(|e| {
            log::error!("[gatt] service list: {e}");
            TransportError::Net
        })?;
        log::debug!("[gatt] {} service(s) advertised", services.len());
        let mut target = None;
        for s in services {
            let Ok(uuid) = s.uuid().await else { continue };
            if uuid == self.service_uuid {
                target = Some(s);
                break;
            }
        }
        let service = target.ok_or_else(|| {
            log::error!("[gatt] service {} not found on device", self.service_uuid);
            TransportError::NotFound
        })?;
        log::debug!("[gatt] service found");

        let chars = service.characteristics().await.map_err(|e| {
            log::error!("[gatt] characteristics: {e}");
            TransportError::Net
        })?;
        log::debug!("[gatt] {} characteristic(s)", chars.len());

        let mut write_char = None;
        let mut notify_char = None;
        for c in chars {
            let Ok(flags) = c.flags().await else { continue };
            log::debug!(
                "[gatt] char {:04x}: write={} write_without_resp={} notify={} indicate={}",
                c.id(),
                flags.write,
                flags.write_without_response,
                flags.notify,
                flags.indicate,
            );
            if write_char.is_none() && (flags.write || flags.write_without_response) {
                write_char = Some(c.clone());
            }
            if notify_char.is_none() && (flags.notify || flags.indicate) {
                notify_char = Some(c);
            }
        }
        let write_char = write_char.ok_or_else(|| {
            log::error!("[gatt] no writable characteristic found");
            TransportError::NotFound
        })?;
        let notify_char = notify_char.ok_or_else(|| {
            log::error!("[gatt] no notifiable characteristic found");
            TransportError::NotFound
        })?;
        log::debug!("[gatt] write char {:04x}, notify char {:04x}", write_char.id(), notify_char.id());

        // Bridge notifications into the read pipe.
        let rx = self.rx.clone();
        self.notify_task = Some(tokio::spawn(async move {
            let stream = match notify_char.notify().await {
                Ok(s) => s,
                Err(e) => {
                    log::error!("[gatt] notify subscribe: {e}");
                    return;
                }
            };
            let mut stream = std::pin::pin!(stream);
            while let Some(value) = stream.next().await {
                if !rx.push(&value) {
                    break;
                }
            }
        }));

        self.write_char = Some(write_char);
        self.connected = true;
        log::info!("[gatt] connected to {mac}");
        Ok(())
    }

    async fn disconnect(&mut self) {
        if let Some(task) = self.notify_task.take() {
            task.abort();
        }
        self.write_char = None;
        self.rx.close();
        self.connected = false;
    }

    async fn poll_read(&mut self, timeout: Duration) -> Result<PollStatus, TransportError> {
        if !self.connected {
            return Err(TransportError::Closed);
        }
        if !self.rx.buf.lock().unwrap().is_empty() {
            return Ok(PollStatus::Ready);
        }
        tokio::time::timeout(timeout, self.rx.notify.notified())
            .await
            .map(|_| PollStatus::Ready)
            .map_err(|_| TransportError::Timeout)
    }

    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        loop {
            if let Some(n) = self.rx.pop(buf) {
                return Ok(n);
            }
            if self.rx.closed.load(std::sync::atomic::Ordering::SeqCst) {
                return Ok(0);
            }
            self.rx.notify.notified().await;
        }
    }

    async fn send(&mut self, data: &[u8]) -> Result<usize, TransportError> {
        let c = self.write_char.as_ref().ok_or(TransportError::Closed)?;
        c.write(data).await.map_err(|e| {
            log::warn!("GATT write: {e}");
            TransportError::Net
        })?;
        Ok(data.len())
    }

    fn connected(&self) -> bool {
        self.connected
    }

    fn kind(&self) -> TransportKind {
        TransportKind::Ble
    }
}
