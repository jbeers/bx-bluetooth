## Problem Statement

BoxLang needs a Matchbox-compatible Bluetooth module that can be used by applications which run in the browser via WASM, on native hosts, and eventually on ESP32 devices. The immediate application-level need is sending printer commands over Bluetooth, but the module itself must remain transport-focused and not absorb printer-specific concerns such as TSPL generation, image preprocessing, or printer-vendor heuristics.

Today there is a Rust proof of concept that mixes printer command construction, Bluetooth transport, and target-specific assumptions in one program. That is useful for validation, but it is the wrong abstraction boundary for a reusable Matchbox module. The reusable module must expose an honest BLE-centric API, preserve clear separation from printer logic, fit Matchbox native module conventions, and leave room for future backend implementations without locking the public API to one platform's quirks.

## Solution

Build a new Matchbox-compatible module named `bx-bluetooth` with public namespace `bluetooth`. The module will be BLE-first and transport-only. It will expose async BoxLang APIs for adapter bootstrap, target-appropriate device discovery, connecting to a device, explicit GATT discovery, characteristic selection, writing binary data, and cleanup.

The module will use Matchbox native classes for stateful resources:

- `Adapter`
- `Device`
- `Connection`
- `Service`
- `Characteristic`
- `CharacteristicSelector`
- `BTError`

The design intentionally separates discovery semantics from portable post-selection behavior:

- Browser WASM exposes `requestDevice(...)`
- Native backends expose `scan(...)`
- Shared portable behavior starts once the caller has a `Device`

The module is designed for browser WASM first, native second, and ESP32 third. The implementation will use a backend split from the start so public Matchbox-facing classes remain stable while each target integrates with its own Bluetooth stack.

## User Stories

1. As a BoxLang developer, I want to include a Bluetooth module in a Matchbox application, so that I can access BLE devices from BoxLang.
2. As a BoxLang developer targeting the browser, I want to request a BLE device through a browser-compatible API, so that my application follows Web Bluetooth constraints.
3. As a BoxLang developer targeting native hosts, I want to scan for BLE devices, so that I can discover nearby peripherals without a browser chooser flow.
4. As a BoxLang developer, I want adapter bootstrap methods to be async, so that my Bluetooth workflow has one consistent future-based model.
5. As a BoxLang developer, I want a thin `Device` object, so that discovery identity is separate from connection state.
6. As a BoxLang developer, I want to connect to a discovered device, so that I can begin GATT operations.
7. As a BoxLang developer, I want a convenience method that connects and performs full GATT discovery, so that I can take a simple happy path when I do not need fine-grained control.
8. As a BoxLang developer, I want explicit `discoverServices()` and `discoverCharacteristics()` methods, so that I can control when discovery cost is paid.
9. As a BoxLang developer, I want service and characteristic objects represented as classes, so that stateful BLE resources feel natural to use in BoxLang.
10. As a BoxLang developer, I want characteristic metadata normalized across targets, so that I can choose a writable characteristic without backend-specific branching.
11. As a BoxLang developer, I want a selector API for characteristics, so that I can filter discovered GATT data without writing repetitive traversal code.
12. As a BoxLang developer, I want the selector API to remain cache-based and side-effect free, so that list operations do not secretly perform Bluetooth I/O.
13. As a BoxLang developer, I want characteristic selection to return arrays, so that application code can enforce its own cardinality policy.
14. As a BoxLang developer, I want writes to accept binary data rather than strings or number arrays, so that transport APIs are strongly typed around bytes.
15. As a BoxLang developer, I want `write()` to handle BLE chunking internally, so that applications do not need to manage packet sizing heuristics.
16. As a BoxLang developer, I want to choose `withResponse` or `withoutResponse`, so that the transport mode is explicit and backend-agnostic.
17. As a BoxLang developer, I want disconnected objects to fail deterministically, so that stale service and characteristic references do not behave unpredictably.
18. As a BoxLang developer, I want `disconnect()` to be idempotent, so that cleanup code is easy to write.
19. As a BoxLang developer, I want repeated discovery calls to be cached and idempotent, so that I do not accidentally trigger rediscovery and object refresh complexity.
20. As a BoxLang developer, I want unsupported target-specific discovery methods omitted where they do not make sense, so that the API surface stays honest.
21. As a BoxLang developer, I want structured Bluetooth errors, so that application code can branch on stable error codes instead of parsing raw strings.
22. As a BoxLang developer, I want `BTError.details` available for diagnostics, so that debugging retains backend-specific context without turning that context into API contract.
23. As a BoxLang developer, I want printer-oriented applications to send bytes produced elsewhere through the Bluetooth module, so that printer command generation stays in a separate module.
24. As a Matchbox module author, I want backend implementations to be split by target, so that public classes are not polluted with `cfg(...)` logic and platform branches.
25. As a Matchbox VM maintainer, I want the Bluetooth module to rely on generic runtime primitives for binary values and native async futures, so that those mechanisms can be reused by future native modules.
26. As a BoxLang developer, I want module examples for browser and native workflows, so that I can adopt the API quickly.
27. As a BoxLang developer, I want one printer-adjacent example that stops at sending bytes, so that module boundaries remain clear.
28. As a maintainer, I want selector logic testable without live Bluetooth hardware, so that core behavior can be validated cheaply and consistently.
29. As a maintainer, I want backend contract tests and smoke tests separated from selector logic tests, so that hardware and runtime constraints do not make the whole test suite brittle.
30. As a maintainer, I want the PRD to include a target matrix, so that browser, native, and ESP32 expectations remain explicit.

## Implementation Decisions

- The module name is `bx-bluetooth` and the public BoxLang namespace is `bluetooth`.
- The module is BLE-only in v1. Classic Bluetooth is deferred.
- The module remains transport-focused and does not include TSPL generation, printer detection logic, image conversion, or printer-vendor shortcuts.
- Public APIs are future-based for adapter access, discovery, connection, discovery, writing, and disconnect.
- The initial implementation order is browser WASM, then native host, then ESP32.
- Device discovery is intentionally not fully portable in v1:
  - Browser uses `requestDevice(...)`
  - Native uses `scan(...)`
  - Shared portable behavior begins after a `Device` has been selected
- The module-level surface is minimal and limited to bootstrap entry points:
  - `getAdapters()`
  - `getDefaultAdapter()`
- Stateful resources are modeled as native classes:
  - `Adapter`
  - `Device`
  - `Connection`
  - `Service`
  - `Characteristic`
  - `CharacteristicSelector`
  - `BTError`
- `Device` is intentionally thin. It exposes identity/basic metadata plus `connect()` and `connectAndDiscover()`.
- `Connection` is the canonical owner of discovered GATT state and caches. `Service` and `Characteristic` are views onto connection-owned state.
- `connectAndDiscover()` is a convenience wrapper that performs:
  - connect
  - full service discovery
  - full characteristic discovery for each service
  - returns a cache-ready `Connection`
- `discoverServices()` and `discoverCharacteristics()` are explicit and truthful names because they may trigger real backend I/O.
- Repeated discovery calls are idempotent cache-returning operations in v1. There is no refresh API.
- Selector behavior is cache-relative:
  - it operates only on already discovered characteristics
  - it does not trigger hidden I/O
  - it may return partial results if discovery has been partial
- `CharacteristicSelector` is a mutable fluent builder created from `connection.selectCharacteristics()`.
- V1 selector filters are intentionally write-focused:
  - `service(uuid)`
  - `uuid(uuid)`
  - `writable()`
  - `writeWithoutResponse()`
  - `writeWithResponse()`
  - terminal: `list()`
- The application owns cardinality policy and characteristic choice. The module does not auto-select one matching characteristic.
- `Characteristic` metadata in v1 is limited to `uuid` and `properties`.
- `properties` is a fixed BoxLang struct of booleans rather than a dedicated class or raw backend flags.
- `Characteristic.write()` accepts the planned Matchbox binary primitive as its public contract.
- `write()` accepts arbitrarily sized logical payloads and handles backend-specific chunking internally.
- `disconnect()` is idempotent.
- `Device.connect()` always attempts a fresh new session. Reconnect helpers are out of scope.
- Service and characteristic objects are invalidated when their parent connection disconnects.
- Pairing, bonding, MTU exposure, read/notify subscriptions, explicit refresh, reconnect helpers, and Classic Bluetooth are out of scope for v1.
- Unsupported target-specific methods may be omitted entirely where they do not make sense. `scan()` is the canonical example.
- `BTError` is a public BoxLang-facing object with stable `code` and `message` fields plus diagnostic-only `details`.
- Error code definitions are centralized so all backends map into the same public contract.
- The Rust implementation is split into target backends from day one:
  - WASM backend
  - native backend
  - ESP32 backend
- The Matchbox-facing wrapper layer stays thin and separate from backend implementation details.
- The Bluetooth module depends on Matchbox VM enhancements for:
  - a general-purpose binary value
  - native async future creation/resolution/rejection

## Testing Decisions

- Good tests validate externally visible behavior and documented contracts rather than internal data layout or backend implementation details.
- Selector and filter behavior should be tested without live Bluetooth hardware using normalized fake discovered data.
- Binary and error marshalling should be tested at the VM boundary so the module can rely on stable runtime behavior.
- Backend tests should focus on contract behavior for each target rather than duplicating selector logic tests.
- Automated tests should not require a live printer.
- Expected testing layers:
  - unit tests for `CharacteristicSelector`
  - unit tests for `BTError` mapping and code normalization
  - VM-level tests for the binary primitive and native async future interop
  - backend contract or smoke tests for browser, native, and later ESP32 paths where feasible
- Similar prior art should be taken from Matchbox tests that already cover heap values, futures, native objects, and JS interop.

## Out of Scope

- Classic Bluetooth / RFCOMM
- Printer-specific command generation
- TSPL or ESC/POS construction
- Image preprocessing for printers
- Printer-vendor UUID shortcuts or printer heuristics
- Pairing or bond management APIs
- MTU inspection or tuning APIs
- Read operations or notification subscriptions
- Discovery refresh APIs
- Reconnect helpers
- Full parity guarantees for ESP32 on day one
- A single fully portable device-discovery API across browser and native in v1

## Further Notes

- Browser WASM is the source of truth for shaping the portable API, but native host support is implemented second to provide faster real-device debugging and validation.
- ESP32 is an intended target with deferred backend validation. Its exact v1 coverage must be called out explicitly in the target matrix.
- Documentation should prioritize narrow examples over broad narrative description.
- One printer-adjacent example is appropriate, but it must treat payload construction as external to `bx-bluetooth`.
