# Matchbox VM Spec for `bx-bluetooth`

## Purpose

`bx-bluetooth` requires two reusable Matchbox VM capabilities that should be implemented in the VM rather than inside the module:

1. A general-purpose binary value for byte-oriented APIs
2. A native async future interop contract so Rust native modules can create, resolve, and reject BoxLang futures safely

This spec treats both as reusable runtime features, with Bluetooth as the forcing use case.

## Goals

- Add a first-class BoxLang binary value suitable for transport, file, network, and interop use cases
- Allow native modules to create BoxLang futures and complete them later
- Keep async completion and scheduler integration owned by the VM
- Preserve GC safety across pending async operations
- Provide a clean path for WASM promise bridging without making Bluetooth special

## Non-Goals

- Redesign the full BoxLang async language surface
- Add streaming abstractions in the VM
- Add Bluetooth-specific logic to the VM
- Add Classic Bluetooth support

## Existing Runtime Facts

Relevant current Matchbox behavior observed in `matchbox-vm`:

- `BxFuture` already exists as a heap-backed GC object
- The interpreter and `spawn()` create pending `Future` objects internally
- The VM exposes `future_on_error(...)` but not a native-module API for creating or settling futures
- WASM JS values and JS promises already appear in the runtime, but native modules do not yet have a clean generic contract for promise-to-future bridging
- Heap object kinds currently include `String`, `Array`, `Struct`, `Instance`, `Future`, `NativeObject`, and target-specific JS values/handles

The recommended implementation should extend these mechanisms, not replace them.

## Part 1: Binary Value

### Public Language Model

Add a first-class BoxLang binary value. It should be documented as a general-purpose byte container, not a Bluetooth-specific type.

Recommended properties:

- ordered bytes
- each element is `0..255`
- mutable only through explicit binary APIs
- not interchangeable with strings
- distinct from generic arrays

### Recommended Runtime Representation

Implement the binary value as a heap object, not as a new NaN-boxing tag in `BxValue` for the first version.

Recommended representation:

- add `GcObject::Bytes(Vec<u8>)`
- continue representing the value in `BxValue` as a pointer-backed heap object

Rationale:

- avoids destabilizing the NaN-boxing layout immediately
- keeps GC integration straightforward
- allows first-class language semantics without making boxed bytes a generic `Array<BxValue>`

This is still a first-class language value even though it is pointer-backed internally.

### VM API Additions

Add binary-oriented methods to the `BxVM` trait. Exact names can vary, but the capability set should cover:

- `bytes_new(&mut self, data: Vec<u8>) -> usize`
- `bytes_len(&self, id: usize) -> usize`
- `bytes_get(&self, id: usize, idx: usize) -> Result<u8, String>`
- `bytes_set(&mut self, id: usize, idx: usize, value: u8) -> Result<(), String>` if mutability is supported in v1
- `to_bytes(&self, val: BxValue) -> Result<Vec<u8>, String>`
- `is_bytes(&self, val: BxValue) -> bool` or equivalent helper

The minimum requirement for `bx-bluetooth` is:

- creation from native Rust
- validation that a `BxValue` is binary
- conversion to `Vec<u8>`

### GC Changes

Update GC traversal to treat `GcObject::Bytes(Vec<u8>)` like strings:

- no child tracing required
- no special root behavior beyond ordinary pointer reachability

Update any debugging/stringification code so bytes produce a recognizable representation, for example:

- `<bytes len:42>`

### WASM / JS Interop

Binary values should bridge cleanly in WASM:

- `bx_to_js(binary)` should produce `Uint8Array`
- `js_to_bx(Uint8Array)` should produce the new binary value
- plain JS arrays may continue to map to BoxLang arrays, not binary

Do not silently coerce BoxLang strings to binary.

### Serialization / JSON

Do not overdesign JSON serialization in the first pass. Recommended behavior:

- JSON conversion may encode binary as an error, `null`, or an explicit representation, but it must be documented
- do not silently stringify arbitrary bytes as UTF-8

If serialization support is needed later, add it intentionally.

### Acceptance Criteria for Binary Value

- Native code can create a binary value and return it as `BxValue`
- Native code can validate and extract bytes from a binary `BxValue`
- Binary values are distinct from arrays and strings in runtime behavior
- GC handles binary values correctly
- WASM JS interop can round-trip `Uint8Array`
- The value is documented as a general-purpose VM feature

## Part 2: Native Async Future Interop

### Public Runtime Requirement

Native Rust modules need a way to return a BoxLang future immediately and settle it later from asynchronous work.

The VM should own:

- future allocation
- settlement
- rejection
- scheduler wake-up
- GC rooting rules for pending async operations
- WASM promise bridging helpers where applicable

Native modules should own:

- starting backend-specific async work
- converting backend results/errors into BoxLang values or `BTError`
- invoking VM settlement APIs

### Recommended VM API Additions

Add native-module-facing future APIs to `BxVM`. Exact names can vary, but the capability set should include:

- `future_new(&mut self) -> BxValue`
- `future_resolve(&mut self, future: BxValue, value: BxValue) -> Result<(), String>`
- `future_reject(&mut self, future: BxValue, error: BxValue) -> Result<(), String>`

Also add a safe scheduling hook so async completions can be applied on the VM's execution context:

- `schedule_native_completion(...)`
- or a generic queued callback/event mechanism owned by the VM

The important part is not the exact method name. The important part is that native code must not mutate VM heap state from arbitrary foreign async callbacks without going through a VM-owned scheduling path.

### Error Semantics

The current `FutureStatus::Failed(String)` shape is too narrow for structured errors.

Recommended change:

- evolve future failure payloads from `String` to `BxValue`
- allow rejection with a BoxLang-visible object such as `BTError`

If changing `FutureStatus` immediately is too invasive, an interim option is:

- store a failure `BxValue` on the future
- preserve a string message only for legacy paths

But the target state should support structured rejection values.

### Scheduler / Event Loop Model

The VM already has cooperative fibers and pending futures. Extend that model rather than introducing a second one.

Recommended behavior:

- native async completion enqueues a settlement event
- the VM applies that event on the next scheduler turn
- any fibers waiting on `future.get()` observe the updated status/value

This keeps heap mutation, error propagation, and wake-up logic centralized.

### GC and Rooting Requirements

Pending async operations must not lose references needed for completion.

The VM should define how pending native async work keeps alive:

- the future object itself
- any native state object needed to complete it
- any captured BoxLang values needed when the completion runs

Recommended approach:

- require native async state holders to implement tracing through an existing native object or VM-managed pending-operation registry
- do not rely on ad hoc manual root stacks in module code

### WASM Promise Bridging

For WASM, the VM should provide a generic way to attach JS promise resolution/rejection to a BoxLang future.

Recommended helper shape:

- create BoxLang future
- attach JS promise `then/catch`
- schedule resolve/reject through the VM's async completion path

Bluetooth should not own this glue uniquely. Any native module using Web APIs should be able to reuse it.

### Threading / Safety

Even on native targets, the API should assume the VM controls when heap mutation happens.

Do not allow:

- direct heap mutation from background threads
- native modules calling `future_resolve` from arbitrary threads unless the VM explicitly marshals it back safely

The spec does not mandate one internal synchronization design, but it does require that the public native-module contract be safe by construction.

### Suggested Rollout

#### Phase 1

- Add `GcObject::Bytes(Vec<u8>)`
- Add binary helpers to `BxVM`
- Add `future_new`, `future_resolve`, `future_reject`
- Add a VM-owned completion queue for native async settlement
- Keep the scope minimal enough for `bx-bluetooth`

#### Phase 2

- Extend WASM JS bridging to round-trip `Uint8Array`
- Add VM helpers for JS promise to BoxLang future settlement
- Improve debug output and docs

#### Phase 3

- Generalize the same native async machinery for other modules
- Consider richer binary APIs if real users need them

## Open Decisions to Resolve During Implementation

- Whether binary mutability is needed in v1 or read-only values are enough
- The exact `FutureStatus` representation for structured rejection values
- Whether to expose binary indexing methods immediately at the language level or start with native-module support only
- The exact shape of the VM-owned async completion queue
- Whether promise bridging should be exposed as a generic helper on the VM trait or implemented inside the WASM VM backend behind the same trait methods

## Recommended Acceptance Criteria

The VM work is complete enough for `bx-bluetooth` when all of the following are true:

- A native module can allocate and return a BoxLang binary value
- A native module can validate and extract bytes from a BoxLang binary value
- A native module can allocate a BoxLang future, return it immediately, and settle it later
- Rejections can carry a structured BoxLang-visible error object
- WASM can bridge `Uint8Array` and JS promises into the new runtime mechanisms
- The resulting APIs are documented as generic Matchbox VM capabilities, not Bluetooth-specific hacks
