#[cfg(all(not(target_arch = "wasm32"), not(target_os = "espidf")))]
pub mod native;

#[cfg(target_os = "espidf")]
pub mod esp32;

#[cfg(target_arch = "wasm32")]
pub mod wasm;
