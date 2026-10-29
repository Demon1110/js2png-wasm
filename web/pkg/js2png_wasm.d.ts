/* tslint:disable */
/* eslint-disable */

/**
 * 反向揭秘：PNG（无密钥编码）-> 原始 JS 源码字符串
 */
export function decode_png(png: Uint8Array): string;

/**
 * 反向揭秘：PNG（带密钥编码）-> 原始 JS 源码字符串
 */
export function decode_png_with_key(png: Uint8Array, key: string): string;

/**
 * 正向编码：JS 源码 -> 加密 PNG（无密钥，仅压缩）
 */
export function encode_js(js: string): Uint8Array;

/**
 * 正向编码：JS 源码 -> 加密 PNG（带密钥）
 */
export function encode_js_with_key(js: string, key: string): Uint8Array;

/**
 * 从 PNG 字节中读取图像宽高（Uint32Array [width, height]）
 */
export function png_dimensions(png: Uint8Array): Uint32Array;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly decode_png: (a: number, b: number) => [number, number, number, number];
    readonly decode_png_with_key: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly encode_js: (a: number, b: number) => [number, number, number, number];
    readonly encode_js_with_key: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly png_dimensions: (a: number, b: number) => [number, number, number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
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
