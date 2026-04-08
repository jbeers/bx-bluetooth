/* tslint:disable */
/* eslint-disable */

export class PrinterHarnessVM {
    free(): void;
    [Symbol.dispose](): void;
    configure(name_prefix: string, optional_services: Array<any>, preferred_characteristic: string, write_mode: string): any;
    connect_and_discover(): Promise<any>;
    disconnect_printer(): Promise<any>;
    get_state(): any;
    constructor();
    payload_length(): number;
    request_printer(): Promise<any>;
    select_characteristic(index: number): any;
    send_test_print(): Promise<any>;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_printerharnessvm_free: (a: number, b: number) => void;
    readonly printerharnessvm_configure: (a: number, b: number, c: number, d: any, e: number, f: number, g: number, h: number) => [number, number, number];
    readonly printerharnessvm_connect_and_discover: (a: number) => any;
    readonly printerharnessvm_disconnect_printer: (a: number) => any;
    readonly printerharnessvm_get_state: (a: number) => [number, number, number];
    readonly printerharnessvm_new: () => [number, number, number];
    readonly printerharnessvm_payload_length: (a: number) => [number, number, number];
    readonly printerharnessvm_request_printer: (a: number) => any;
    readonly printerharnessvm_select_characteristic: (a: number, b: number) => [number, number, number];
    readonly printerharnessvm_send_test_print: (a: number) => any;
    readonly wasm_bindgen__convert__closures_____invoke__hefe4d67b8089656b: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h1980d0da418a5b8e: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h1980d0da418a5b8e_2: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h1980d0da418a5b8e_3: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h1980d0da418a5b8e_4: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h62b68375f778a8be: (a: number, b: number, c: any, d: any) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
