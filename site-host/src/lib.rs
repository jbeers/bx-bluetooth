use bx_bluetooth_native::{register_bifs, register_classes};
use js_sys::Array;
use matchbox_compiler::{compiler::Compiler, parser};
use matchbox_vm::{types::BxValue, vm::VM, Chunk};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

fn as_js_error(message: impl Into<String>) -> JsValue {
    JsValue::from_str(&message.into())
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
        let source = include_str!("../../site/printer_harness.bxs");
        let ast = parser::parse(source).map_err(|e| as_js_error(format!("Parse Error: {}", e)))?;
        let compiler = Compiler::new("site/printer_harness.bxs");
        let mut chunk = compiler
            .compile(&ast, source)
            .map_err(|e| as_js_error(format!("Compiler Error: {}", e)))?;

        chunk.reconstruct_functions();
        let chunk_rc = Rc::new(RefCell::new(chunk.clone()));

        let mut vm = VM::new_with_bifs(register_bifs(), register_classes());
        vm.interpret(chunk)
            .map_err(|e| as_js_error(format!("VM Runtime Error: {}", e)))?;

        Ok(PrinterHarnessVM {
            vm,
            chunk: Some(chunk_rc),
        })
    }

    pub fn call(&mut self, name: &str, args: Array) -> Result<JsValue, JsValue> {
        let mut bx_args = Vec::new();
        for idx in 0..args.length() {
            bx_args.push(self.vm.js_to_bx(args.get(idx)));
        }

        let func = self
            .vm
            .get_global(name)
            .ok_or_else(|| as_js_error(format!("Function {} not found", name)))?;

        let value = self
            .vm
            .call_function_value(func, bx_args, self.chunk.clone())
            .map_err(|e| as_js_error(format!("VM Runtime Error: {}", e)))?;

        Ok(self.vm.bx_to_js(&value))
    }

    pub fn get_state(&mut self) -> Result<JsValue, JsValue> {
        self.call("getState", Array::new())
    }

    pub fn payload_length(&mut self) -> Result<f64, JsValue> {
        let state = self.call("getState", Array::new())?;
        let payload_length = js_sys::Reflect::get(&state, &JsValue::from_str("payloadLength"))
            .map_err(|_| as_js_error("Failed to read payloadLength"))?;
        payload_length
            .as_f64()
            .ok_or_else(|| as_js_error("payloadLength was not numeric"))
    }
}
