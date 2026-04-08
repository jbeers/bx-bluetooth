use esp32_nimble::{
    utilities::{mutex::Mutex, BleUuid},
    BLEAddress, BLEAdvertisedDevice, BLEClient, BLEDevice,
    BLERemoteCharacteristic, BLERemoteService, BLEScan,
};
use esp_idf_svc::hal::task::block_on;
use std::ffi::CString;
use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

#[derive(Clone, Debug, Default)]
pub struct AdapterHandle;

#[derive(Clone, Debug)]
pub struct DeviceHandle {
    pub address: BLEAddress,
    pub id: String,
    pub name: String,
}

#[derive(Clone)]
pub struct ConnectionHandle {
    pub client: Arc<Mutex<BLEClient>>,
}

impl std::fmt::Debug for ConnectionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionHandle").finish()
    }
}

#[derive(Clone, Debug)]
pub struct CharacteristicHandle {
    pub inner: BLERemoteCharacteristic,
    pub service_uuid: String,
    pub uuid: String,
    pub write: bool,
    pub write_without_response: bool,
}

#[derive(Clone, Debug)]
pub struct ServiceHandle {
    pub inner: BLERemoteService,
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

struct WorkerTask<T> {
    job: Option<Box<dyn FnOnce() -> Result<T, String> + Send>>,
    tx: mpsc::SyncSender<Result<T, String>>,
}

fn run_on_worker_task<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = mpsc::sync_channel(1);
    let task = Box::new(WorkerTask {
        job: Some(Box::new(job)),
        tx,
    });

    extern "C" fn task_entry<T: Send + 'static>(param: *mut std::ffi::c_void) {
        let mut task = unsafe { Box::from_raw(param.cast::<WorkerTask<T>>()) };
        let result = task
            .job
            .take()
            .expect("worker task job missing")();
        let _ = task.tx.send(result);
        unsafe { esp_idf_svc::sys::vTaskDelete(std::ptr::null_mut()) };
    }

    let name = CString::new("bx_ble_worker").map_err(|error| error.to_string())?;
    let res = unsafe {
        esp_idf_svc::sys::xTaskCreatePinnedToCore(
            Some(task_entry::<T>),
            name.as_ptr(),
            48 * 1024,
            Box::into_raw(task).cast(),
            5,
            std::ptr::null_mut(),
            0,
        )
    };
    if res != 1 {
        return Err(format!("failed to create BLE worker task ({})", res));
    }
    rx.recv().map_err(|error| error.to_string())?
}

fn init_ble_runtime() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        esp_idf_svc::sys::link_patches();
    });
}

fn ble_device() -> &'static BLEDevice {
    init_ble_runtime();
    BLEDevice::take()
}

fn normalize_ble_uuid(uuid: BleUuid) -> String {
    uuid.to_string().to_lowercase()
}

fn service_filter_matches(_device: &BLEAdvertisedDevice, _filters: &[Uuid]) -> bool {
    // TODO: `esp32-nimble` scan callbacks provide advertisement data that can be
    // inspected for service UUIDs. Wire that through once the ESP32 toolchain is
    // available locally to validate the exact API shape.
    true
}

fn characteristic_flags(characteristic: &BLERemoteCharacteristic) -> (bool, bool) {
    // TODO: validate the exact remote-characteristic property API under the ESP-IDF
    // toolchain. For now, prefer permissive defaults so discovered characteristics
    // remain selectable during the first integration pass.
    let _ = characteristic;
    (true, true)
}

pub fn get_adapters() -> Result<Vec<AdapterHandle>, String> {
    init_ble_runtime();
    Ok(vec![AdapterHandle])
}

pub fn get_default_adapter() -> Result<AdapterHandle, String> {
    init_ble_runtime();
    Ok(AdapterHandle)
}

pub fn scan(_adapter: &AdapterHandle, options: &ScanOptions) -> Result<Vec<DeviceHandle>, String> {
    let timeout_ms = options.timeout_ms.max(1);
    let name_prefix = options.name_prefix.clone();
    let service_filters = options.services.clone();
    run_on_worker_task(move || {
        let found = Arc::new(Mutex::new(Vec::<DeviceHandle>::new()));
        let found_ref = Arc::clone(&found);

        block_on(async move {
            let mut ble_scan = BLEScan::new();
            let _ = ble_scan
                .active_scan(true)
                .interval(100)
                .window(99)
                .start(ble_device(), timeout_ms as _, move |device, data| {
                    let name = data.name().map(|name| name.to_string()).unwrap_or_default();

                    if !service_filters.is_empty() && !service_filter_matches(device, &service_filters) {
                        return None::<()>;
                    }

                    let id = format!("{:?}", device.addr());
                    let mut devices = found_ref.lock();
                    if let Some(existing) = devices.iter_mut().find(|existing| existing.id == id) {
                        if existing.name.is_empty() && !name.is_empty() {
                            existing.name = name;
                        }
                        return None::<()>;
                    }

                    devices.push(DeviceHandle { address: device.addr(), id, name });
                    None::<()>
                })
                .await
                .map_err(|error| error.to_string())?;

            let mut devices = found.lock().clone();
            if let Some(prefix) = &name_prefix {
                devices.retain(|device| device.name.starts_with(prefix));
            }
            Ok(devices)
        })
    })
}

pub fn connect(device: &DeviceHandle) -> Result<ConnectionHandle, String> {
    let address = device.address;
    block_on(async move {
        let client = Arc::new(Mutex::new(ble_device().new_client()));
        {
            let mut locked = client.lock();
            locked
                .connect(&address)
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok(ConnectionHandle { client })
    })
}

pub fn disconnect(connection: &ConnectionHandle) -> Result<(), String> {
    let mut client = connection.client.lock();
    client.disconnect().map_err(|error| error.to_string())
}

pub fn discover_services(connection: &ConnectionHandle) -> Result<Vec<ServiceHandle>, String> {
    block_on(async {
        let mut client = connection.client.lock();
        let services = client
            .get_services()
            .await
            .map_err(|error| error.to_string())?;

        let mut out = Vec::new();
        for service in services {
            let service_uuid = normalize_ble_uuid(service.uuid());
            let mut characteristics_out = Vec::new();
            let characteristics = service
                .get_characteristics()
                .await
                .map_err(|error| error.to_string())?;

            for characteristic in characteristics {
                let (write, write_without_response) = characteristic_flags(characteristic);
                characteristics_out.push(CharacteristicHandle {
                    service_uuid: service_uuid.clone(),
                    uuid: normalize_ble_uuid(characteristic.uuid()),
                    write,
                    write_without_response,
                    inner: characteristic.clone(),
                });
            }

            out.push(ServiceHandle {
                uuid: service_uuid,
                primary: true,
                characteristics: characteristics_out,
                inner: service.clone(),
            });
        }

        Ok(out)
    })
}

pub fn write(
    _connection: &ConnectionHandle,
    characteristic: &CharacteristicHandle,
    data: &[u8],
    mode: WriteMode,
) -> Result<(), String> {
    let mut characteristic = characteristic.inner.clone();
    block_on(async move {
        characteristic
            .write_value(data, matches!(mode, WriteMode::WithResponse))
            .await
            .map_err(|error| error.to_string())
    })
}
