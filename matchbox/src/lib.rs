use matchbox_macros::bx_methods;
use matchbox_vm::types::{BxNativeFunction, BxNativeObject, BxVM, BxValue, Tracer};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use uuid::Uuid;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;
#[cfg(target_arch = "wasm32")]
use matchbox_vm::types::{register_wasm_future_thunk, NativeFutureValue};

mod backend;

#[cfg(not(target_arch = "wasm32"))]
use backend::native as platform;
#[cfg(target_arch = "wasm32")]
use backend::wasm as platform;

#[derive(Clone, Debug)]
struct CharacteristicRecord {
    backend: platform::CharacteristicHandle,
    service_uuid: String,
    uuid: String,
    write: bool,
    write_without_response: bool,
    uuid_value: BxValue,
    properties_value: BxValue,
    object_value: Option<BxValue>,
}

#[derive(Clone, Debug)]
struct ServiceRecord {
    uuid: String,
    primary: bool,
    uuid_value: BxValue,
    object_value: Option<BxValue>,
    characteristics_discovered: bool,
    characteristics: Vec<CharacteristicRecord>,
}

#[derive(Clone, Debug)]
struct ConnectionState {
    live: bool,
    backend: platform::ConnectionHandle,
    services_discovered: bool,
    services: Vec<ServiceRecord>,
}

#[derive(Debug)]
struct BTError {
    code: BxValue,
    message: BxValue,
    details: BxValue,
}

#[derive(Debug)]
struct Adapter {
    backend: platform::AdapterHandle,
}

#[derive(Debug)]
struct Device {
    backend: platform::DeviceHandle,
    id: BxValue,
    name: BxValue,
}

#[derive(Debug)]
struct Connection {
    state: Rc<RefCell<ConnectionState>>,
}

#[derive(Debug)]
struct Service {
    state: Rc<RefCell<ConnectionState>>,
    service_uuid: String,
    uuid: BxValue,
    primary: bool,
}

#[derive(Debug)]
struct Characteristic {
    state: Rc<RefCell<ConnectionState>>,
    service_uuid: String,
    characteristic_uuid: String,
    uuid: BxValue,
    properties: BxValue,
}

#[derive(Clone, Debug)]
struct SelectorEntry {
    service_uuid: String,
    characteristic_uuid: String,
    write: bool,
    write_without_response: bool,
    object_value: BxValue,
}

#[derive(Debug)]
struct CharacteristicSelector {
    entries: Vec<SelectorEntry>,
    service_filter: Option<String>,
    uuid_filter: Option<String>,
    require_write: bool,
    require_write_without_response: bool,
    require_write_with_response: bool,
}

fn string_value(vm: &mut dyn BxVM, value: impl Into<String>) -> BxValue {
    BxValue::new_ptr(vm.string_new(value.into()))
}

fn array_from_values(vm: &mut dyn BxVM, values: &[BxValue]) -> BxValue {
    let id = vm.array_new();
    for value in values {
        vm.array_push(id, *value);
    }
    BxValue::new_ptr(id)
}

fn normalized_uuid(input: &str) -> Result<String, String> {
    parse_uuidish(input).map(|uuid| uuid.to_string().to_lowercase())
}

fn parse_uuidish(input: &str) -> Result<Uuid, String> {
    let raw = input.trim().to_lowercase();
    let expanded = match raw.len() {
        4 => format!("0000{}-0000-1000-8000-00805f9b34fb", raw),
        8 => format!("{}-0000-1000-8000-00805f9b34fb", raw),
        _ => raw,
    };
    Uuid::parse_str(&expanded).map_err(|_| format!("Invalid UUID: {}", input))
}

fn parse_string_array(vm: &dyn BxVM, val: BxValue) -> Result<Vec<String>, String> {
    let id = val.as_gc_id().ok_or_else(|| "Expected array".to_string())?;
    let len = vm.array_len(id);
    let mut out = Vec::with_capacity(len);
    for idx in 0..len {
        out.push(vm.to_string(vm.array_get(id, idx)));
    }
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_scan_options(vm: &dyn BxVM, args: &[BxValue]) -> Result<platform::ScanOptions, String> {
    let mut options = platform::ScanOptions {
        timeout_ms: 3000,
        services: Vec::new(),
        name_prefix: None,
    };

    if args.is_empty() || args[0].is_null() {
        return Ok(options);
    }

    let id = args[0]
        .as_gc_id()
        .ok_or_else(|| "scan options must be a struct".to_string())?;

    if vm.struct_key_exists(id, "timeout") {
        options.timeout_ms = vm.struct_get(id, "timeout").as_number() as u64;
    }

    if vm.struct_key_exists(id, "namePrefix") {
        options.name_prefix = Some(vm.to_string(vm.struct_get(id, "namePrefix")));
    }

    if vm.struct_key_exists(id, "services") {
        let service_values = parse_string_array(vm, vm.struct_get(id, "services"))?;
        options.services = service_values
            .iter()
            .map(|service| parse_uuidish(service))
            .collect::<Result<Vec<_>, _>>()?;
    }

    Ok(options)
}

#[cfg(target_arch = "wasm32")]
fn parse_request_device_options(
    vm: &dyn BxVM,
    options: BxValue,
) -> Result<platform::RequestDeviceOptionsInput, String> {
    let mut parsed = platform::RequestDeviceOptionsInput::default();

    if options.is_null() {
        return Ok(parsed);
    }

    let id = options
        .as_gc_id()
        .ok_or_else(|| "requestDevice options must be a struct".to_string())?;

    if vm.struct_key_exists(id, "services") {
        parsed.services = parse_string_array(vm, vm.struct_get(id, "services"))?
            .into_iter()
            .map(|service| normalized_uuid(&service))
            .collect::<Result<Vec<_>, _>>()?;
    }

    if vm.struct_key_exists(id, "optionalServices") {
        parsed.optional_services = parse_string_array(vm, vm.struct_get(id, "optionalServices"))?
            .into_iter()
            .map(|service| normalized_uuid(&service))
            .collect::<Result<Vec<_>, _>>()?;
    }

    if vm.struct_key_exists(id, "namePrefix") {
        parsed.name_prefix = Some(vm.to_string(vm.struct_get(id, "namePrefix")));
    }

    Ok(parsed)
}

fn make_properties_value(vm: &mut dyn BxVM, write: bool, write_without_response: bool) -> BxValue {
    let id = vm.struct_new();
    vm.struct_set(id, "write", BxValue::new_bool(write));
    vm.struct_set(
        id,
        "writeWithoutResponse",
        BxValue::new_bool(write_without_response),
    );
    BxValue::new_ptr(id)
}

fn make_bt_error(
    vm: &mut dyn BxVM,
    code: impl Into<String>,
    message: impl Into<String>,
    details: Option<BxValue>,
) -> BxValue {
    let error = BTError {
        code: string_value(vm, code.into()),
        message: string_value(vm, message.into()),
        details: details.unwrap_or_else(BxValue::new_null),
    };
    let id = vm.native_object_new(Rc::new(RefCell::new(error)));
    BxValue::new_ptr(id)
}

fn resolved_future(vm: &mut dyn BxVM, value: BxValue) -> Result<BxValue, String> {
    let future = vm.future_new();
    vm.future_resolve(future, value)?;
    Ok(future)
}

fn rejected_future(vm: &mut dyn BxVM, error: BxValue) -> Result<BxValue, String> {
    let future = vm.future_new();
    vm.future_reject(future, error)?;
    Ok(future)
}

fn future_from_result<T>(
    vm: &mut dyn BxVM,
    result: Result<T, String>,
    on_ok: impl FnOnce(&mut dyn BxVM, T) -> Result<BxValue, String>,
) -> Result<BxValue, String> {
    match result {
        Ok(value) => {
            let settled = on_ok(vm, value)?;
            resolved_future(vm, settled)
        }
        Err(message) => {
            let error = make_bt_error(vm, "backendError", message, None);
            rejected_future(vm, error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn wasm_future_from_async<T: 'static>(
    vm: &mut dyn BxVM,
    fut: impl std::future::Future<Output = Result<T, String>> + 'static,
    on_ok: impl FnOnce(&mut dyn BxVM, T) -> Result<BxValue, String> + 'static,
) -> Result<BxValue, String> {
    let handle = vm.native_future_new();
    let future = handle.future();

    spawn_local(async move {
        match fut.await {
            Ok(value) => {
                let thunk_id = register_wasm_future_thunk(Box::new(move |vm| on_ok(vm, value)));
                let _ = handle.resolve_wasm_thunk(thunk_id);
            }
            Err(message) => {
                let _ = handle.reject(NativeFutureValue::Error { message });
            }
        }
    });

    Ok(future)
}

fn ensure_live(state: &Rc<RefCell<ConnectionState>>) -> Result<(), String> {
    if !state.borrow().live {
        return Err("connection is closed".to_string());
    }
    Ok(())
}

fn create_adapter_value(vm: &mut dyn BxVM, backend: platform::AdapterHandle) -> BxValue {
    let id = vm.native_object_new(Rc::new(RefCell::new(Adapter { backend })));
    BxValue::new_ptr(id)
}

fn create_device_value(vm: &mut dyn BxVM, backend: platform::DeviceHandle) -> BxValue {
    let id_value = string_value(vm, backend.id.clone());
    let name_value = string_value(vm, backend.name.clone());
    let device = Device {
        backend,
        id: id_value,
        name: name_value,
    };
    let id = vm.native_object_new(Rc::new(RefCell::new(device)));
    BxValue::new_ptr(id)
}

fn create_connection_value(vm: &mut dyn BxVM, backend: platform::ConnectionHandle) -> BxValue {
    let state = Rc::new(RefCell::new(ConnectionState {
        live: true,
        backend,
        services_discovered: false,
        services: Vec::new(),
    }));
    create_connection_value_from_state(vm, state)
}

fn create_connection_value_from_state(
    vm: &mut dyn BxVM,
    state: Rc<RefCell<ConnectionState>>,
) -> BxValue {
    let connection = Connection { state };
    let id = vm.native_object_new(Rc::new(RefCell::new(connection)));
    BxValue::new_ptr(id)
}

fn service_value_from_record(
    vm: &mut dyn BxVM,
    state: Rc<RefCell<ConnectionState>>,
    uuid: &str,
    uuid_value: BxValue,
    primary: bool,
) -> BxValue {
    let service = Service {
        state,
        service_uuid: uuid.to_string(),
        uuid: uuid_value,
        primary,
    };
    let id = vm.native_object_new(Rc::new(RefCell::new(service)));
    BxValue::new_ptr(id)
}

fn characteristic_value_from_record(
    vm: &mut dyn BxVM,
    state: Rc<RefCell<ConnectionState>>,
    service_uuid: &str,
    characteristic_uuid: &str,
    uuid_value: BxValue,
    properties_value: BxValue,
) -> BxValue {
    let characteristic = Characteristic {
        state,
        service_uuid: service_uuid.to_string(),
        characteristic_uuid: characteristic_uuid.to_string(),
        uuid: uuid_value,
        properties: properties_value,
    };
    let id = vm.native_object_new(Rc::new(RefCell::new(characteristic)));
    BxValue::new_ptr(id)
}

fn populate_services(
    vm: &mut dyn BxVM,
    state: &Rc<RefCell<ConnectionState>>,
    services: Vec<platform::ServiceHandle>,
    with_characteristics: bool,
) -> Vec<BxValue> {
    let mut state_mut = state.borrow_mut();
    if state_mut.services_discovered {
        return state_mut
            .services
            .iter()
            .filter_map(|service| service.object_value)
            .collect();
    }

    let mut service_values = Vec::new();
    for service in services {
        let uuid_value = string_value(vm, service.uuid.clone());
        let service_obj = service_value_from_record(
            vm,
            Rc::clone(state),
            &service.uuid,
            uuid_value,
            service.primary,
        );

        let mut record = ServiceRecord {
            uuid: service.uuid.clone(),
            primary: service.primary,
            uuid_value,
            object_value: Some(service_obj),
            characteristics_discovered: with_characteristics,
            characteristics: Vec::new(),
        };

        if with_characteristics {
            for characteristic in service.characteristics {
                let uuid_value = string_value(vm, characteristic.uuid.clone());
                let properties_value = make_properties_value(
                    vm,
                    characteristic.write,
                    characteristic.write_without_response,
                );
                let char_obj = characteristic_value_from_record(
                    vm,
                    Rc::clone(state),
                    &characteristic.service_uuid,
                    &characteristic.uuid,
                    uuid_value,
                    properties_value,
                );

                record.characteristics.push(CharacteristicRecord {
                    backend: characteristic.clone(),
                    service_uuid: characteristic.service_uuid.clone(),
                    uuid: characteristic.uuid.clone(),
                    write: characteristic.write,
                    write_without_response: characteristic.write_without_response,
                    uuid_value,
                    properties_value,
                    object_value: Some(char_obj),
                });
            }
        } else {
            for characteristic in service.characteristics {
                let uuid = characteristic.uuid.clone();
                let write = characteristic.write;
                let write_without_response = characteristic.write_without_response;
                let uuid_value = string_value(vm, characteristic.uuid.clone());
                let properties_value = make_properties_value(
                    vm,
                    characteristic.write,
                    characteristic.write_without_response,
                );
                record.characteristics.push(CharacteristicRecord {
                    backend: characteristic,
                    service_uuid: service.uuid.clone(),
                    uuid,
                    write,
                    write_without_response,
                    uuid_value,
                    properties_value,
                    object_value: None,
                });
            }
        }

        service_values.push(service_obj);
        state_mut.services.push(record);
    }
    state_mut.services_discovered = true;
    service_values
}

fn uuid_value_to_string(vm: &dyn BxVM, value: BxValue) -> String {
    vm.to_string(value).to_lowercase()
}

fn discover_characteristics_for_service(
    vm: &mut dyn BxVM,
    state: &Rc<RefCell<ConnectionState>>,
    service_uuid: &str,
) -> Result<Vec<BxValue>, String> {
    ensure_live(state)?;
    let mut state_mut = state.borrow_mut();
    let service = state_mut
        .services
        .iter_mut()
        .find(|service| service.uuid == service_uuid)
        .ok_or_else(|| format!("Service {} not found", service_uuid))?;

    if service.characteristics_discovered {
        return Ok(service
            .characteristics
            .iter()
            .filter_map(|characteristic| characteristic.object_value)
            .collect());
    }

    let mut values = Vec::new();
    for characteristic in &mut service.characteristics {
        let char_obj = characteristic_value_from_record(
            vm,
            Rc::clone(state),
            &characteristic.service_uuid,
            &characteristic.uuid,
            characteristic.uuid_value,
            characteristic.properties_value,
        );
        characteristic.object_value = Some(char_obj);
        values.push(char_obj);
    }

    service.characteristics_discovered = true;
    Ok(values)
}

fn connection_selector_entries(
    state: &Rc<RefCell<ConnectionState>>,
) -> Result<Vec<SelectorEntry>, String> {
    ensure_live(state)?;
    let state_ref = state.borrow();
    let mut entries = Vec::new();
    for service in &state_ref.services {
        for characteristic in &service.characteristics {
            if let Some(object_value) = characteristic.object_value {
                entries.push(SelectorEntry {
                    service_uuid: characteristic.service_uuid.clone(),
                    characteristic_uuid: characteristic.uuid.clone(),
                    write: characteristic.write,
                    write_without_response: characteristic.write_without_response,
                    object_value,
                });
            }
        }
    }
    if entries.is_empty() {
        return Err("No characteristics have been discovered yet".to_string());
    }
    Ok(entries)
}

fn parse_write_mode(vm: &dyn BxVM, args: &[BxValue]) -> platform::WriteMode {
    if args.len() < 2 || args[1].is_null() {
        return platform::WriteMode::WithoutResponse;
    }

    if let Some(id) = args[1].as_gc_id() {
        if vm.struct_key_exists(id, "mode") {
            let mode = vm.to_string(vm.struct_get(id, "mode")).to_lowercase();
            if mode == "withresponse" {
                return platform::WriteMode::WithResponse;
            }
        }
    }

    platform::WriteMode::WithoutResponse
}

fn get_object_property(name: &str, mappings: &[(&str, BxValue)]) -> BxValue {
    for (key, value) in mappings {
        if key.eq_ignore_ascii_case(name) {
            return *value;
        }
    }
    BxValue::new_null()
}

impl BxNativeObject for BTError {
    fn get_property(&self, name: &str) -> BxValue {
        get_object_property(
            name,
            &[
                ("code", self.code),
                ("message", self.message),
                ("details", self.details),
            ],
        )
    }

    fn set_property(&mut self, _name: &str, _value: BxValue) {}

    fn call_method(
        &mut self,
        _vm: &mut dyn BxVM,
        _id: usize,
        name: &str,
        _args: &[BxValue],
    ) -> Result<BxValue, String> {
        Err(format!("Method {} not found", name))
    }

    fn trace(&self, tracer: &mut dyn Tracer) {
        tracer.mark(&self.code);
        tracer.mark(&self.message);
        tracer.mark(&self.details);
    }
}

impl Adapter {
    pub fn scan(&mut self, vm: &mut dyn BxVM, options: BxValue) -> Result<BxValue, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
        let args = [options];
        let parsed = parse_scan_options(vm, &args)?;
            return future_from_result(vm, platform::scan(&self.backend, &parsed), |vm, devices| {
                let values: Vec<BxValue> = devices
                    .into_iter()
                    .map(|device| create_device_value(vm, device))
                    .collect();
                Ok(array_from_values(vm, &values))
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            let error = make_bt_error(
                vm,
                "notSupported",
                "scan() is not supported on browser WASM targets",
                None,
            );
            let _ = options;
            return rejected_future(vm, error);
        }
    }

    pub fn request_device(&mut self, vm: &mut dyn BxVM, options: BxValue) -> Result<BxValue, String> {
        #[cfg(target_arch = "wasm32")]
        {
            let parsed = parse_request_device_options(vm, options)?;
            let backend = self.backend.clone();
            return wasm_future_from_async(vm, async move { platform::request_device(&backend, &parsed).await }, |vm, device| {
                Ok(create_device_value(vm, device))
            });
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let error = make_bt_error(
                vm,
                "notSupported",
                "requestDevice() is only supported on browser WASM targets",
                None,
            );
            let _ = options;
            return rejected_future(vm, error);
        }
    }
}

#[bx_methods]
impl Adapter {
    pub fn bx_scan(&mut self, vm: &mut dyn BxVM, options: BxValue) -> Result<BxValue, String> {
        self.scan(vm, options)
    }

    pub fn requestDevice(&mut self, vm: &mut dyn BxVM, options: BxValue) -> Result<BxValue, String> {
        self.request_device(vm, options)
    }
}

impl BxNativeObject for Adapter {
    fn get_property(&self, _name: &str) -> BxValue {
        BxValue::new_null()
    }

    fn set_property(&mut self, _name: &str, _value: BxValue) {}

    fn call_method(
        &mut self,
        vm: &mut dyn BxVM,
        id: usize,
        name: &str,
        args: &[BxValue],
    ) -> Result<BxValue, String> {
        self.dispatch_method(vm, id, name, args)
    }
}

#[bx_methods]
impl Device {
    pub fn connect(&mut self, vm: &mut dyn BxVM) -> Result<BxValue, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            return future_from_result(vm, platform::connect(&self.backend), |vm, connection| {
                Ok(create_connection_value(vm, connection))
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            let backend = self.backend.clone();
            return wasm_future_from_async(vm, async move { platform::connect(&backend).await }, |vm, connection| {
                Ok(create_connection_value(vm, connection))
            });
        }
    }

    pub fn connectAndDiscover(&mut self, vm: &mut dyn BxVM) -> Result<BxValue, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            return match platform::connect(&self.backend) {
                Ok(connection_handle) => {
                    let state = Rc::new(RefCell::new(ConnectionState {
                        live: true,
                        backend: connection_handle.clone(),
                        services_discovered: false,
                        services: Vec::new(),
                    }));
                    let services = platform::discover_services(&connection_handle)?;
                    populate_services(vm, &state, services, true);
                    let connection_value = create_connection_value_from_state(vm, state);
                    resolved_future(vm, connection_value)
                }
                Err(message) => {
                    let error = make_bt_error(vm, "connectionFailed", message, None);
                    rejected_future(vm, error)
                }
            };
        }

        #[cfg(target_arch = "wasm32")]
        {
            let backend = self.backend.clone();
            return wasm_future_from_async(
                vm,
                async move {
                    let connection = platform::connect(&backend).await?;
                    let services = platform::discover_services(&connection).await?;
                    Ok((connection, services))
                },
                |vm, (connection_handle, services)| {
                    let state = Rc::new(RefCell::new(ConnectionState {
                        live: true,
                        backend: connection_handle,
                        services_discovered: false,
                        services: Vec::new(),
                    }));
                    populate_services(vm, &state, services, true);
                    Ok(create_connection_value_from_state(vm, state))
                },
            );
        }
    }
}

impl BxNativeObject for Device {
    fn get_property(&self, name: &str) -> BxValue {
        get_object_property(name, &[("id", self.id), ("name", self.name)])
    }

    fn set_property(&mut self, _name: &str, _value: BxValue) {}

    fn call_method(
        &mut self,
        vm: &mut dyn BxVM,
        id: usize,
        name: &str,
        args: &[BxValue],
    ) -> Result<BxValue, String> {
        self.dispatch_method(vm, id, name, args)
    }

    fn trace(&self, tracer: &mut dyn Tracer) {
        tracer.mark(&self.id);
        tracer.mark(&self.name);
    }
}

#[bx_methods]
impl Connection {
    pub fn discoverServices(&mut self, vm: &mut dyn BxVM) -> Result<BxValue, String> {
        ensure_live(&self.state)?;
        if self.state.borrow().services_discovered {
            let values: Vec<BxValue> = self
                .state
                .borrow()
                .services
                .iter()
                .filter_map(|service| service.object_value)
                .collect();
            let array = array_from_values(vm, &values);
            return resolved_future(vm, array);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            return future_from_result(
                vm,
                platform::discover_services(&self.state.borrow().backend),
                |vm, services| {
                    let values = populate_services(vm, &self.state, services, false);
                    Ok(array_from_values(vm, &values))
                },
            );
        }

        #[cfg(target_arch = "wasm32")]
        {
            let backend = self.state.borrow().backend.clone();
            let state = Rc::clone(&self.state);
            return wasm_future_from_async(vm, async move { platform::discover_services(&backend).await }, move |vm, services| {
                let values = populate_services(vm, &state, services, false);
                Ok(array_from_values(vm, &values))
            });
        }
    }

    pub fn selectCharacteristics(&mut self, vm: &mut dyn BxVM) -> Result<BxValue, String> {
        match connection_selector_entries(&self.state) {
            Ok(entries) => {
                let selector = CharacteristicSelector {
                    entries,
                    service_filter: None,
                    uuid_filter: None,
                    require_write: false,
                    require_write_without_response: false,
                    require_write_with_response: false,
                };
                let id = vm.native_object_new(Rc::new(RefCell::new(selector)));
                Ok(BxValue::new_ptr(id))
            }
            Err(message) => Err(message),
        }
    }

    pub fn disconnect(&mut self, vm: &mut dyn BxVM) -> Result<BxValue, String> {
        if !self.state.borrow().live {
            return resolved_future(vm, BxValue::new_null());
        }

        match platform::disconnect(&self.state.borrow().backend) {
            Ok(()) => {
                self.state.borrow_mut().live = false;
                resolved_future(vm, BxValue::new_null())
            }
            Err(message) => {
                let error = make_bt_error(vm, "backendError", message, None);
                rejected_future(vm, error)
            }
        }
    }
}

impl BxNativeObject for Connection {
    fn get_property(&self, _name: &str) -> BxValue {
        BxValue::new_null()
    }

    fn set_property(&mut self, _name: &str, _value: BxValue) {}

    fn call_method(
        &mut self,
        vm: &mut dyn BxVM,
        id: usize,
        name: &str,
        args: &[BxValue],
    ) -> Result<BxValue, String> {
        self.dispatch_method(vm, id, name, args)
    }

    fn trace(&self, tracer: &mut dyn Tracer) {
        let state = self.state.borrow();
        for service in &state.services {
            tracer.mark(&service.uuid_value);
            if let Some(obj) = service.object_value {
                tracer.mark(&obj);
            }
            for characteristic in &service.characteristics {
                tracer.mark(&characteristic.uuid_value);
                tracer.mark(&characteristic.properties_value);
                if let Some(obj) = characteristic.object_value {
                    tracer.mark(&obj);
                }
            }
        }
    }
}

#[bx_methods]
impl Service {
    pub fn discoverCharacteristics(&mut self, vm: &mut dyn BxVM) -> Result<BxValue, String> {
        match discover_characteristics_for_service(vm, &self.state, &self.service_uuid) {
            Ok(values) => {
                let array = array_from_values(vm, &values);
                resolved_future(vm, array)
            }
            Err(message) => {
                let error = make_bt_error(vm, "invalidState", message, None);
                rejected_future(vm, error)
            }
        }
    }
}

impl BxNativeObject for Service {
    fn get_property(&self, name: &str) -> BxValue {
        match name.to_lowercase().as_str() {
            "uuid" => self.uuid,
            "primary" => BxValue::new_bool(self.primary),
            _ => BxValue::new_null(),
        }
    }

    fn set_property(&mut self, _name: &str, _value: BxValue) {}

    fn call_method(
        &mut self,
        vm: &mut dyn BxVM,
        id: usize,
        name: &str,
        args: &[BxValue],
    ) -> Result<BxValue, String> {
        self.dispatch_method(vm, id, name, args)
    }

    fn trace(&self, tracer: &mut dyn Tracer) {
        tracer.mark(&self.uuid);
    }
}

#[bx_methods]
impl Characteristic {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn write(
        &mut self,
        vm: &mut dyn BxVM,
        data: BxValue,
        options: BxValue,
    ) -> Result<BxValue, String> {
        ensure_live(&self.state)?;
        let bytes = vm
            .to_bytes(data)
            .map_err(|message| format!("write expects binary data: {}", message))?;
        let mode = parse_write_mode(vm, &[data, options]);

        let state = self.state.borrow();
        let characteristic = state
            .services
            .iter()
            .find(|service| service.uuid == self.service_uuid)
            .and_then(|service| {
                service
                    .characteristics
                    .iter()
                    .find(|characteristic| characteristic.uuid == self.characteristic_uuid)
            })
            .ok_or_else(|| "Characteristic not found".to_string())?;

        match platform::write(&state.backend, &characteristic.backend, &bytes, mode) {
            Ok(()) => resolved_future(vm, BxValue::new_null()),
            Err(message) => {
                let error = make_bt_error(vm, "writeFailed", message, None);
                rejected_future(vm, error)
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn write(
        &mut self,
        vm: &mut dyn BxVM,
        data: BxValue,
        options: BxValue,
    ) -> Result<BxValue, String> {
        ensure_live(&self.state)?;
        let bytes = vm
            .to_bytes(data)
            .map_err(|message| format!("write expects binary data: {}", message))?;
        let mode = parse_write_mode(vm, &[data, options]);

        let state = self.state.borrow();
        let backend = state.backend.clone();
        let characteristic = state
            .services
            .iter()
            .find(|service| service.uuid == self.service_uuid)
            .and_then(|service| {
                service
                    .characteristics
                    .iter()
                    .find(|characteristic| characteristic.uuid == self.characteristic_uuid)
            })
            .ok_or_else(|| "Characteristic not found".to_string())?
            .backend
            .clone();
        drop(state);

        wasm_future_from_async(
            vm,
            async move { platform::write(&backend, &characteristic, &bytes, mode).await.map(|_| ()) },
            |_vm, ()| Ok(BxValue::new_null()),
        )
    }
}

impl BxNativeObject for Characteristic {
    fn get_property(&self, name: &str) -> BxValue {
        get_object_property(
            name,
            &[("uuid", self.uuid), ("properties", self.properties)],
        )
    }

    fn set_property(&mut self, _name: &str, _value: BxValue) {}

    fn call_method(
        &mut self,
        vm: &mut dyn BxVM,
        id: usize,
        name: &str,
        args: &[BxValue],
    ) -> Result<BxValue, String> {
        self.dispatch_method(vm, id, name, args)
    }

    fn trace(&self, tracer: &mut dyn Tracer) {
        tracer.mark(&self.uuid);
        tracer.mark(&self.properties);
    }
}

#[bx_methods]
impl CharacteristicSelector {
    pub fn service(&mut self, service_uuid: String) -> &mut Self {
        self.service_filter = normalized_uuid(&service_uuid).ok();
        self
    }

    pub fn uuid(&mut self, characteristic_uuid: String) -> &mut Self {
        self.uuid_filter = normalized_uuid(&characteristic_uuid).ok();
        self
    }

    pub fn writable(&mut self) -> &mut Self {
        self.require_write = true;
        self
    }

    pub fn writeWithoutResponse(&mut self) -> &mut Self {
        self.require_write_without_response = true;
        self
    }

    pub fn writeWithResponse(&mut self) -> &mut Self {
        self.require_write_with_response = true;
        self
    }

    pub fn list(&mut self, vm: &mut dyn BxVM) -> BxValue {
        let values: Vec<BxValue> = self
            .entries
            .iter()
            .filter(|entry| {
                self.service_filter
                    .as_ref()
                    .map(|filter| &entry.service_uuid == filter)
                    .unwrap_or(true)
            })
            .filter(|entry| {
                self.uuid_filter
                    .as_ref()
                    .map(|filter| &entry.characteristic_uuid == filter)
                    .unwrap_or(true)
            })
            .filter(|entry| !self.require_write || entry.write)
            .filter(|entry| !self.require_write_without_response || entry.write_without_response)
            .filter(|entry| {
                !self.require_write_with_response || (entry.write && !entry.write_without_response)
            })
            .map(|entry| entry.object_value)
            .collect();

        array_from_values(vm, &values)
    }
}

impl BxNativeObject for CharacteristicSelector {
    fn get_property(&self, _name: &str) -> BxValue {
        BxValue::new_null()
    }

    fn set_property(&mut self, _name: &str, _value: BxValue) {}

    fn call_method(
        &mut self,
        vm: &mut dyn BxVM,
        id: usize,
        name: &str,
        args: &[BxValue],
    ) -> Result<BxValue, String> {
        self.dispatch_method(vm, id, name, args)
    }

    fn trace(&self, tracer: &mut dyn Tracer) {
        for entry in &self.entries {
            tracer.mark(&entry.object_value);
        }
    }
}

fn get_adapters(vm: &mut dyn BxVM, _args: &[BxValue]) -> Result<BxValue, String> {
    future_from_result(vm, platform::get_adapters(), |vm, adapters| {
        let values: Vec<BxValue> = adapters
            .into_iter()
            .map(|adapter| create_adapter_value(vm, adapter))
            .collect();
        Ok(array_from_values(vm, &values))
    })
}

fn get_default_adapter(vm: &mut dyn BxVM, _args: &[BxValue]) -> Result<BxValue, String> {
    future_from_result(vm, platform::get_default_adapter(), |vm, adapter| {
        Ok(create_adapter_value(vm, adapter))
    })
}

fn create_bt_error(vm: &mut dyn BxVM, args: &[BxValue]) -> Result<BxValue, String> {
    if args.len() < 2 {
        return Err("BTError requires code and message".to_string());
    }
    let details = if args.len() > 2 { Some(args[2]) } else { None };
    let code = vm.to_string(args[0]);
    let message = vm.to_string(args[1]);
    Ok(make_bt_error(vm, code, message, details))
}

pub fn register_bifs() -> HashMap<String, BxNativeFunction> {
    let mut map = HashMap::new();
    map.insert("getadapters".to_string(), get_adapters as BxNativeFunction);
    map.insert(
        "getdefaultadapter".to_string(),
        get_default_adapter as BxNativeFunction,
    );
    map
}

pub fn register_classes() -> HashMap<String, BxNativeFunction> {
    let mut map = HashMap::new();
    map.insert(
        "bx_bluetooth_native.BTError".to_string(),
        create_bt_error as BxNativeFunction,
    );
    map
}

#[cfg(test)]
mod tests {
    use super::{normalized_uuid, parse_uuidish};

    #[test]
    fn parses_short_16_bit_uuid() {
        let uuid = parse_uuidish("2af1").expect("16-bit UUID should parse");
        assert_eq!(uuid.to_string(), "00002af1-0000-1000-8000-00805f9b34fb");
    }

    #[test]
    fn normalizes_full_uuid_to_lowercase() {
        let normalized =
            normalized_uuid("00002AF1-0000-1000-8000-00805F9B34FB").expect("UUID should normalize");
        assert_eq!(normalized, "00002af1-0000-1000-8000-00805f9b34fb");
    }
}
