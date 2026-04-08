# `bx-bluetooth` Draft Documentation

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

This is a design-first documentation draft for the planned API. The implementation is expected to target:

1. browser WASM
2. native host
3. ESP32

The final module should include a target matrix documenting exact availability by environment.

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

These are the only module-level entry points planned for v1.

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

Planned public properties:

- `id`
- `name`

Planned public methods:

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

Planned public fields:

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

## Examples

See:

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
