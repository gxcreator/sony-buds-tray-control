//! Device discovery over BlueZ D-Bus (mirrors `dbus_list_adapters` from the
//! reference client).

use super::TransportError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub name: String,
    pub mac: String,
    pub paired: bool,
    pub connected: bool,
}

/// Lists candidate devices. Implemented by [`BlueZDeviceLister`]; mocked in
/// tests.
#[async_trait::async_trait]
pub trait DeviceLister: Send + Sync {
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>, TransportError>;
}

pub struct BlueZDeviceLister {
    session: bluer::Session,
}

impl BlueZDeviceLister {
    pub fn new(session: bluer::Session) -> Self {
        Self { session }
    }
}

#[async_trait::async_trait]
impl DeviceLister for BlueZDeviceLister {
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>, TransportError> {
        let adapter = self.session.default_adapter().await.map_err(|e| {
            log::warn!("no default Bluetooth adapter: {e}");
            TransportError::NotFound
        })?;
        let addresses = adapter.device_addresses().await.map_err(|e| {
            log::warn!("adapter device addresses: {e}");
            TransportError::Net
        })?;
        let mut out = Vec::with_capacity(addresses.len());
        for addr in addresses {
            let Ok(device) = adapter.device(addr) else {
                continue;
            };
            let name = device.name().await.unwrap_or_default().unwrap_or_default();
            let paired = device.is_paired().await.unwrap_or(false);
            let connected = device.is_connected().await.unwrap_or(false);
            out.push(DeviceInfo {
                name,
                mac: device.address().to_string(),
                paired,
                connected,
            });
        }
        Ok(out)
    }
}

/// Test double: fixed device list.
pub struct StaticDeviceLister(pub Vec<DeviceInfo>);

#[async_trait::async_trait]
impl DeviceLister for StaticDeviceLister {
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>, TransportError> {
        Ok(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn static_lister_returns_fixed_list() {
        let lister = StaticDeviceLister(vec![DeviceInfo {
            name: "WH-1000XM5".into(),
            mac: "AA:BB:CC:DD:EE:FF".into(),
            paired: true,
            connected: false,
        }]);
        let list = lister.list_devices().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "WH-1000XM5");
    }
}
