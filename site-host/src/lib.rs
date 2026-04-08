use bx_bluetooth_native::{register_bifs, register_classes};
use console_error_panic_hook;
use js_sys::{Array, Promise};
use matchbox_compiler::{compiler::Compiler, parser};
use matchbox_vm::{vm::VM, Chunk};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::window;

fn as_js_error(message: impl Into<String>) -> JsValue {
    JsValue::from_str(&message.into())
}

async fn yield_to_host() -> Result<(), JsValue> {
    let promise = Promise::new(&mut |resolve, reject| {
        let Some(win) = window() else {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("window is unavailable"));
            return;
        };

        if let Err(err) = win.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 0) {
            let _ = reject.call1(&JsValue::NULL, &err);
        }
    });

    let _ = JsFuture::from(promise).await?;
    Ok(())
}

#[wasm_bindgen]
pub struct PrinterHarnessVM {
    vm: VM,
    chunk: Option<Rc<RefCell<Chunk>>>,
}

#[wasm_bindgen]
impl PrinterHarnessVM {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<PrinterHarnessVM, JsValue> {
        console_error_panic_hook::set_once();

        let source = include_str!("../../site/printer_harness.bxs");
        let ast = parser::parse(source).map_err(|e| as_js_error(format!("Parse Error: {}", e)))?;
        let compiler = Compiler::new("site/printer_harness.bxs");
        let mut chunk = compiler
            .compile(&ast, source)
            .map_err(|e| as_js_error(format!("Compiler Error: {}", e)))?;

        chunk.reconstruct_functions();
        let chunk_rc = Rc::new(RefCell::new(chunk.clone()));

        let mut vm = VM::new_with_bifs(register_bifs(), register_classes());
        vm.interpret_no_timeslice(chunk)
            .map_err(|e| as_js_error(format!("VM Runtime Error: {}", e)))?;

        Ok(PrinterHarnessVM {
            vm,
            chunk: Some(chunk_rc),
        })
    }

    fn resolve_function(&mut self, name: &str) -> Result<matchbox_vm::types::BxValue, JsValue> {
        self.vm
            .get_global(name)
            .ok_or_else(|| as_js_error(format!("Function {} not found", name)))
    }

    fn bx_args(&mut self, args: Array) -> Vec<matchbox_vm::types::BxValue> {
        let mut bx_args = Vec::new();
        for idx in 0..args.length() {
            bx_args.push(self.vm.js_to_bx(args.get(idx)));
        }
        bx_args
    }

    fn call_sync_internal(&mut self, name: &str, args: Array) -> Result<JsValue, JsValue> {
        let func = self.resolve_function(name)?;
        let bx_args = self.bx_args(args);
        let future = self
            .vm
            .begin_call_function_value(func, bx_args)
            .map_err(|e| as_js_error(format!("VM Runtime Error: {}", e)))?;
        let value = self
            .vm
            .run_until_future_settled_no_timeslice(future)
            .map_err(|e| as_js_error(format!("VM Runtime Error: {}", e)))?;
        Ok(self.vm.bx_to_js(&value))
    }

    async fn call_async_internal(&mut self, name: &str, args: Array) -> Result<JsValue, JsValue> {
        let func = self.resolve_function(name)?;
        let bx_args = self.bx_args(args);
        let future = self
            .vm
            .begin_call_function_value(func, bx_args)
            .map_err(|e| as_js_error(format!("VM Runtime Error: {}", e)))?;

        loop {
            self.vm
                .pump_once_no_timeslice()
                .map_err(|e| as_js_error(format!("VM Runtime Error: {}", e)))?;

            let (state, value) = self
                .vm
                .future_snapshot(future)
                .map_err(|e| as_js_error(format!("VM Runtime Error: {}", e)))?;

            match state {
                0 => {
                    yield_to_host().await?;
                }
                1 => {
                    let value = value.unwrap_or(matchbox_vm::types::BxValue::new_null());
                    return Ok(self.vm.bx_to_js(&value));
                }
                2 => {
                    let error = value.unwrap_or(matchbox_vm::types::BxValue::new_null());
                    return Err(self.vm.bx_to_js(&error));
                }
                _ => {
                    return Err(as_js_error("Unknown future state"));
                }
            }
        }
    }

    pub fn configure(
        &mut self,
        name_prefix: String,
        optional_services: Array,
        preferred_characteristic: String,
        write_mode: String,
    ) -> Result<JsValue, JsValue> {
        let args = Array::new();
        args.push(&JsValue::from_str(&name_prefix));
        args.push(&optional_services);
        args.push(&JsValue::from_str(&preferred_characteristic));
        args.push(&JsValue::from_str(&write_mode));
        self.call_sync_internal("configure", args)
    }

    pub fn select_characteristic(&mut self, index: u32) -> Result<JsValue, JsValue> {
        let args = Array::new();
        args.push(&JsValue::from_f64(index as f64));
        self.call_sync_internal("selectCharacteristic", args)
    }

    pub async fn request_printer(&mut self) -> Result<JsValue, JsValue> {
        self.call_async_internal("requestPrinter", Array::new()).await
    }

    pub async fn connect_and_discover(&mut self) -> Result<JsValue, JsValue> {
        self.call_async_internal("connectAndDiscover", Array::new()).await
    }

    pub async fn send_test_print(&mut self) -> Result<JsValue, JsValue> {
        self.call_async_internal("sendTestPrint", Array::new()).await
    }

    pub async fn disconnect_printer(&mut self) -> Result<JsValue, JsValue> {
        self.call_async_internal("disconnectPrinter", Array::new()).await
    }

    pub fn get_state(&mut self) -> Result<JsValue, JsValue> {
        self.call_sync_internal("getState", Array::new())
    }

    pub fn payload_length(&mut self) -> Result<f64, JsValue> {
        let state = self.get_state()?;
        let payload_length = js_sys::Reflect::get(&state, &JsValue::from_str("payloadLength"))
            .map_err(|_| as_js_error("Failed to read payloadLength"))?;
        payload_length
            .as_f64()
            .ok_or_else(|| as_js_error("payloadLength was not numeric"))
    }
}
