# `bx-bluetooth`

## Overview

`bx-bluetooth` is a Matchbox-compatible BoxLang module for Bluetooth Low Energy transport.

This module is intentionally transport-focused:

- it discovers devices
- connects to devices
- performs GATT discovery
- helps select characteristics
- writes binary payloads

This module does not generate printer commands, convert images, or contain printer-specific logic. Those concerns belong in companion modules or application code.

## Status

The implementation now has concrete backends for:

1. browser WASM
2. native host
3. ESP32 via ESP-IDF / MatchBox fusion

Current backend matrix:

| Environment | Backend | Status |
|---|---|---|
| Browser / WASM | Web Bluetooth | working |
| Native host | `btleplug` | working |
| ESP32 / ESP-IDF | `esp32-nimble` via MatchBox fusion | working on ESP32-S3 hardware |

The shared BoxLang API is intentionally aligned across those targets, but device discovery remains target-shaped:

- browsers use `requestDevice()`
- native hosts use `scan()`
- ESP32 currently uses the same `scan()` path as native host

## Public Namespace

The module is exposed as:

```boxlang
bluetooth
```

## Design Principles

- BLE only in v1
- future-based APIs for Bluetooth operations
- target-specific discovery methods are only exposed where they make sense
- portable shared behavior begins after a `Device` has been selected
- explicit discovery and cache ownership
- no hidden Bluetooth I/O inside selector queries
- binary payloads, not strings, for writes

## Public API

### Module-Level Bootstrap

```boxlang
bluetooth.getAdapters() -> Future<Array<Adapter>>
bluetooth.getDefaultAdapter() -> Future<Adapter>
```

These are the only module-level entry points in v1.

### `Adapter`

Browser-oriented discovery:

```boxlang
adapter.requestDevice( {
    services: [ "18F0" ],
    optionalServices: [ "180A" ],
    namePrefix: "KM"
} ) -> Future<Device>
```

Native-oriented discovery:

```boxlang
adapter.scan( {
    timeout: 3000,
    services: [ "18F0" ],
    namePrefix: "KM"
} ) -> Future<Array<Device>>
```

`scan()` is target-specific and may be omitted where it does not make sense.

### `Device`

Public properties:

- `id`
- `name`

Public methods:

```boxlang
device.connect() -> Future<Connection>
device.connectAndDiscover() -> Future<Connection>
```

`Device` is intentionally thin. GATT state lives on `Connection`.

### `Connection`

```boxlang
connection.discoverServices() -> Future<Array<Service>>
connection.selectCharacteristics() -> CharacteristicSelector
connection.disconnect() -> Future<void>
```

Rules:

- `connectAndDiscover()` fully populates the connection cache
- repeated discovery calls return cached results in v1
- there is no refresh API in v1
- `disconnect()` is idempotent

### `Service`

Property:

- `uuid`

Method:

```boxlang
service.discoverCharacteristics() -> Future<Array<Characteristic>>
```

### `Characteristic`

Properties:

- `uuid`
- `properties`

`properties` is a fixed boolean struct. Planned write-oriented flags:

```boxlang
{
    write: true,
    writeWithoutResponse: true
}
```

Method:

```boxlang
characteristic.write( data, { mode: "withoutResponse" } ) -> Future<void>
```

`data` is intended to be the Matchbox binary value described in the VM spec.

### `CharacteristicSelector`

Created from:

```boxlang
var selector = connection.selectCharacteristics();
```

Planned fluent filters:

```boxlang
selector.service( "18F0" )
selector.uuid( "2AF1" )
selector.writable()
selector.writeWithoutResponse()
selector.writeWithResponse()
```

Terminal operation:

```boxlang
selector.list() -> Array<Characteristic>
```

Rules:

- selector methods mutate the selector and return `this`
- selector reads only from discovered characteristic cache
- selector does not trigger hidden I/O
- selector results may be partial if discovery has been partial
- application code owns cardinality policy and final characteristic selection

### `BTError`

Public fields:

- `code`
- `message`
- `details`

Contract:

- `code` is stable and intended for control flow
- `message` is stable and human-readable
- `details` is diagnostic-only and should not be relied on for application logic

## Object Lifecycle Rules

- `Device.connect()` always attempts a fresh new connection
- `Connection` owns all discovered GATT cache/state
- `Service` and `Characteristic` are views onto connection-owned state
- after `disconnect()`, all derived `Service` and `Characteristic` objects are invalid
- invalid objects should fail deterministically with a structured Bluetooth error

## Out of Scope for V1

- Classic Bluetooth / RFCOMM
- pairing/bonding APIs
- MTU inspection or tuning
- read operations
- notification subscriptions
- refresh APIs
- reconnect helpers
- printer command generation

## ESP32 Notes

The ESP32 backend lives in [`matchbox/src/backend/esp32.rs`](../matchbox/src/backend/esp32.rs).

It is designed for Matchbox's ESP-IDF-based `--target esp32` path and currently assumes:

- `esp32-nimble` for BLE client operations
- `esp-idf-svc` / `esp-idf-sys` from the Matchbox ESP32 runner stack
- NimBLE enabled in [`matchbox/sdkconfig.defaults`](../matchbox/sdkconfig.defaults)

Current status:

- target-specific Cargo/dependency wiring is in place
- backend selection now routes `target_os = "espidf"` away from the desktop `btleplug` backend
- the ESP32 backend has been validated on real ESP32-S3 hardware through MatchBox's `--target esp32` fusion path
- scan, connect, service discovery, characteristic selection, and printer writes are working end-to-end
- the backend still has TODOs around richer advertisement/service filtering and tighter characteristic property introspection

### MatchBox Workflow

The current ESP32 flow assumes:

1. MatchBox is built on the host toolchain.
2. ESP-IDF is activated in the shell used for `--target esp32`.
3. `RUSTUP_TOOLCHAIN=esp` is set for the actual ESP32 build.

Typical shell setup:

```bash
source /path/to/esp-idf/export.sh
export RUSTUP_TOOLCHAIN=esp
export ESP_IDF_ESPUP_CLANG_SYMLINK=ignore
export LIBCLANG_PATH=/usr/lib64
export PATH="$HOME/.cargo/bin:$PATH"
```

Useful sanity check:

```bash
matchbox esp32-doctor
```

Typical ESP32 smoke-test deploy:

```bash
matchbox \
  /path/to/bx-bluetooth/esp32-printer-smoke.bxs \
  --module /path/to/bx-bluetooth \
  --target esp32 \
  --chip esp32s3 \
  --full-flash
```

If local serial permissions still block flash/monitor access, the current fallback remains manual `espflash`.

### Current ESP32 Caveats

- advertisement-time service filtering is still conservative
- characteristic write capability on ESP32 is still inferred permissively in the backend
- the smoke script is intentionally printer-focused and not yet a polished app-facing helper

## Examples

See:

- [`../esp32-printer-smoke.bxs`](../esp32-printer-smoke.bxs) for the current ESP32 printer smoke test
- [`../matchbox/examples/esp32_scan_harness.rs`](../matchbox/examples/esp32_scan_harness.rs) for a native-only ESP32 scan harness used to isolate backend/runtime issues

- [Browser Request Device](./examples/browser-request-device.bxs)
- [Native Scan](./examples/native-scan.bxs)
- [Connect, Discover, Select, Write](./examples/connect-discover-select-write.bxs)
- [Printer-Adjacent Write](./examples/printer-adjacent-write.bxs)

## Browser Harness

For basic phone-based browser testing, this repo also contains a static browser harness in
[`site/`](../site/).

This harness now runs a small Matchbox-based wasm host from [`site-host/`](../site-host/) and
executes the BoxLang script in [`printer_harness.bxs`](../site/printer_harness.bxs). The goal is
to validate:

- secure-origin browser access
- chooser-based printer selection
- GATT discovery
- writable characteristic selection
- TSPL payload delivery to a BLE thermal printer

The harness uses a hardcoded TSPL sample and defaults to the UUIDs already proven in the Rust POC:

- service UUID hint: `18f0`
- preferred write characteristic: `2af1`

The GitHub Pages workflow in [`deploy-pages.yml`](../.github/workflows/deploy-pages.yml) publishes
the `site/` directory so it can be tested from an HTTPS origin on a phone.

Generated browser artifacts live in [`site/pkg/`](../site/pkg/). Rebuild them from the repo root
with:

```bash
cd site-host
cargo build --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir ../site/pkg --out-name printer_harness \
  target/wasm32-unknown-unknown/release/bx_bluetooth_site_host.wasm
```
