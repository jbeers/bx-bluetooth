use btleplug::api::{
    Central, CharPropFlags, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::runtime::Runtime;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct AdapterHandle {
    pub adapter: Adapter,
}

#[derive(Clone, Debug)]
pub struct DeviceHandle {
    pub peripheral: Peripheral,
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct ConnectionHandle {
    pub peripheral: Peripheral,
}

#[derive(Clone, Debug)]
pub struct CharacteristicHandle {
    pub inner: Characteristic,
    pub service_uuid: String,
    pub uuid: String,
    pub write: bool,
    pub write_without_response: bool,
}

#[derive(Clone, Debug)]
pub struct ServiceHandle {
    pub uuid: String,
    pub primary: bool,
    pub characteristics: Vec<CharacteristicHandle>,
}

#[derive(Clone, Debug, Default)]
pub struct ScanOptions {
    pub timeout_ms: u64,
    pub services: Vec<Uuid>,
    pub name_prefix: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteMode {
    WithResponse,
    WithoutResponse,
}

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("tokio runtime"))
}

pub fn get_adapters() -> Result<Vec<AdapterHandle>, String> {
    runtime().block_on(async {
        let manager = Manager::new().await.map_err(|e| e.to_string())?;
        let adapters = manager.adapters().await.map_err(|e| e.to_string())?;
        Ok(adapters
            .into_iter()
            .map(|adapter| AdapterHandle { adapter })
            .collect())
    })
}

pub fn get_default_adapter() -> Result<AdapterHandle, String> {
    get_adapters()?
        .into_iter()
        .next()
        .ok_or_else(|| "No Bluetooth adapters found".to_string())
}

pub fn scan(adapter: &AdapterHandle, options: &ScanOptions) -> Result<Vec<DeviceHandle>, String> {
    let adapter = adapter.adapter.clone();
    runtime().block_on(async move {
        adapter
            .start_scan(ScanFilter {
                services: options.services.clone(),
            })
            .await
            .map_err(|e| e.to_string())?;

        tokio::time::sleep(Duration::from_millis(options.timeout_ms.max(1))).await;

        let peripherals = adapter.peripherals().await.map_err(|e| e.to_string())?;
        let _ = adapter.stop_scan().await;

        let mut devices = Vec::new();
        for peripheral in peripherals {
            let properties = peripheral.properties().await.map_err(|e| e.to_string())?;
            let name = properties
                .as_ref()
                .and_then(|props| {
                    props
                        .local_name
                        .clone()
                        .or(props.advertisement_name.clone())
                })
                .unwrap_or_default();

            if let Some(prefix) = &options.name_prefix {
                if !name.starts_with(prefix) {
                    continue;
                }
            }

            let id = peripheral.address().to_string();
            devices.push(DeviceHandle {
                peripheral,
                id,
                name,
            });
        }

        Ok(devices)
    })
}

pub fn connect(device: &DeviceHandle) -> Result<ConnectionHandle, String> {
    let peripheral = device.peripheral.clone();
    runtime().block_on(async move {
        peripheral.connect().await.map_err(|e| e.to_string())?;
        Ok(ConnectionHandle { peripheral })
    })
}

pub fn disconnect(connection: &ConnectionHandle) -> Result<(), String> {
    let peripheral = connection.peripheral.clone();
    runtime().block_on(async move { peripheral.disconnect().await.map_err(|e| e.to_string()) })
}

pub fn discover_services(connection: &ConnectionHandle) -> Result<Vec<ServiceHandle>, String> {
    let peripheral = connection.peripheral.clone();
    runtime().block_on(async move {
        peripheral
            .discover_services()
            .await
            .map_err(|e| e.to_string())?;

        let mut services = Vec::new();
        for service in peripheral.services() {
            let characteristics = service
                .characteristics
                .iter()
                .cloned()
                .map(|characteristic| CharacteristicHandle {
                    service_uuid: characteristic.service_uuid.to_string().to_lowercase(),
                    uuid: characteristic.uuid.to_string().to_lowercase(),
                    write: characteristic.properties.contains(CharPropFlags::WRITE),
                    write_without_response: characteristic
                        .properties
                        .contains(CharPropFlags::WRITE_WITHOUT_RESPONSE),
                    inner: characteristic,
                })
                .collect();

            services.push(ServiceHandle {
                uuid: service.uuid.to_string().to_lowercase(),
                primary: service.primary,
                characteristics,
            });
        }

        Ok(services)
    })
}

pub fn write(
    connection: &ConnectionHandle,
    characteristic: &CharacteristicHandle,
    data: &[u8],
    mode: WriteMode,
) -> Result<(), String> {
    let peripheral = connection.peripheral.clone();
    let characteristic = characteristic.inner.clone();
    let chunks: Vec<Vec<u8>> = data.chunks(128).map(|chunk| chunk.to_vec()).collect();

    runtime().block_on(async move {
        let write_type = match mode {
            WriteMode::WithResponse => WriteType::WithResponse,
            WriteMode::WithoutResponse => WriteType::WithoutResponse,
        };

        for chunk in chunks {
            peripheral
                .write(&characteristic, &chunk, write_type)
                .await
                .map_err(|e| e.to_string())?;

            if matches!(mode, WriteMode::WithoutResponse) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }

        Ok(())
    })
}
