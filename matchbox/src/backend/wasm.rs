use js_sys::{Array, JsString};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Bluetooth, BluetoothCharacteristicProperties, BluetoothDevice,
    BluetoothLeScanFilterInit, BluetoothRemoteGattCharacteristic,
    BluetoothRemoteGattServer, BluetoothRemoteGattService, RequestDeviceOptions,
};

#[derive(Clone, Debug)]
pub struct AdapterHandle {
    pub bluetooth: Bluetooth,
}

#[derive(Clone, Debug)]
pub struct DeviceHandle {
    pub device: BluetoothDevice,
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct ConnectionHandle {
    pub server: BluetoothRemoteGattServer,
}

#[derive(Clone, Debug)]
pub struct CharacteristicHandle {
    pub inner: BluetoothRemoteGattCharacteristic,
    pub service_uuid: String,
    pub uuid: String,
    pub write: bool,
    pub write_without_response: bool,
}

#[derive(Clone, Debug)]
pub struct ServiceHandle {
    pub inner: BluetoothRemoteGattService,
    pub uuid: String,
    pub primary: bool,
    pub characteristics: Vec<CharacteristicHandle>,
}

#[derive(Clone, Debug, Default)]
pub struct RequestDeviceOptionsInput {
    pub services: Vec<String>,
    pub optional_services: Vec<String>,
    pub name_prefix: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteMode {
    WithResponse,
    WithoutResponse,
}

fn js_error(error: JsValue) -> String {
    error
        .as_string()
        .unwrap_or_else(|| format!("{:?}", error))
}

fn properties(handle: &BluetoothCharacteristicProperties) -> (bool, bool) {
    (handle.write(), handle.write_without_response())
}

fn navigator_bluetooth() -> Result<Bluetooth, String> {
    let window = web_sys::window().ok_or_else(|| "No browser window is available".to_string())?;
    let navigator = window.navigator();
    navigator
        .bluetooth()
        .ok_or_else(|| "Navigator.bluetooth is unavailable".to_string())
}

pub fn get_adapters() -> Result<Vec<AdapterHandle>, String> {
    Ok(vec![AdapterHandle {
        bluetooth: navigator_bluetooth()?,
    }])
}

pub fn get_default_adapter() -> Result<AdapterHandle, String> {
    Ok(AdapterHandle {
        bluetooth: navigator_bluetooth()?,
    })
}

pub async fn request_device(
    adapter: &AdapterHandle,
    options: &RequestDeviceOptionsInput,
) -> Result<DeviceHandle, String> {
    let request_options = RequestDeviceOptions::new();

    if options.services.is_empty() && options.name_prefix.is_none() {
        request_options.set_accept_all_devices(true);
    } else {
        let filter = BluetoothLeScanFilterInit::new();

        if let Some(name_prefix) = &options.name_prefix {
            filter.set_name_prefix(name_prefix);
        }

        if !options.services.is_empty() {
            let services: Vec<JsString> = options
                .services
                .iter()
                .map(|service| JsString::from(service.as_str()))
                .collect();
            filter.set_services(&services);
        }

        request_options.set_filters(&[filter]);
    }

    if !options.optional_services.is_empty() {
        let optional_services: Vec<JsString> = options
            .optional_services
            .iter()
            .map(|service| JsString::from(service.as_str()))
            .collect();
        request_options.set_optional_services(&optional_services);
    }

    let promise = adapter.bluetooth.request_device(&request_options);
    let device: BluetoothDevice = JsFuture::from(promise).await.map_err(js_error)?;

    Ok(DeviceHandle {
        id: device.id(),
        name: device.name().unwrap_or_default(),
        device,
    })
}

pub async fn connect(device: &DeviceHandle) -> Result<ConnectionHandle, String> {
    let gatt = device
        .device
        .gatt()
        .ok_or_else(|| "Device does not expose a GATT server".to_string())?;
    let promise = gatt.connect();
    let server: BluetoothRemoteGattServer = JsFuture::from(promise).await.map_err(js_error)?;
    Ok(ConnectionHandle { server })
}

pub fn disconnect(connection: &ConnectionHandle) -> Result<(), String> {
    connection.server.disconnect();
    Ok(())
}

pub async fn discover_services(connection: &ConnectionHandle) -> Result<Vec<ServiceHandle>, String> {
    let services_value = JsFuture::from(connection.server.get_primary_services())
        .await
        .map_err(js_error)?;
    let services_array: Array = JsValue::from(services_value).into();

    let mut services = Vec::new();
    for service_value in services_array.iter() {
        let service: BluetoothRemoteGattService = service_value
            .dyn_into()
            .map_err(|_| "Service entry was not a BluetoothRemoteGattService".to_string())?;

        let chars_value = JsFuture::from(service.get_characteristics())
            .await
            .map_err(js_error)?;
        let chars_array: Array = JsValue::from(chars_value).into();

        let mut characteristics = Vec::new();
        for characteristic_value in chars_array.iter() {
            let characteristic: BluetoothRemoteGattCharacteristic = characteristic_value
                .dyn_into()
                .map_err(|_| {
                    "Characteristic entry was not a BluetoothRemoteGattCharacteristic".to_string()
                })?;
            let props = characteristic.properties();
            let (write, write_without_response) = properties(&props);
            characteristics.push(CharacteristicHandle {
                service_uuid: service.uuid().to_lowercase(),
                uuid: characteristic.uuid().to_lowercase(),
                write,
                write_without_response,
                inner: characteristic,
            });
        }

        services.push(ServiceHandle {
            uuid: service.uuid().to_lowercase(),
            primary: service.is_primary(),
            characteristics,
            inner: service,
        });
    }

    Ok(services)
}

pub async fn write(
    _connection: &ConnectionHandle,
    characteristic: &CharacteristicHandle,
    data: &[u8],
    mode: WriteMode,
) -> Result<(), String> {
    let mut bytes = data.to_vec();
    let promise = match mode {
        WriteMode::WithResponse => characteristic
            .inner
            .write_value_with_response_with_u8_slice(&mut bytes)
            .map_err(js_error)?,
        WriteMode::WithoutResponse => characteristic
            .inner
            .write_value_without_response_with_u8_slice(&mut bytes)
            .map_err(js_error)?,
    };

    JsFuture::from(promise).await.map_err(js_error)?;
    Ok(())
}
