/* tslint:disable */
/* eslint-disable */

/**
 * Browser-facing wrapper around the emulator.
 */
export class Emulator {
    free(): void;
    [Symbol.dispose](): void;
    frame(): void;
    /**
     * RGBA pixels, 256x192.
     */
    framebuffer(): Uint8Array;
    constructor(rom: Uint8Array);
    pause(): void;
    reset(): void;
    /**
     * 0 up, 1 down, 2 left, 3 right, 4 button 1, 5 button 2.
     */
    set_button(button: number, down: boolean): void;
    /**
     * Mono f32 samples at 44100 Hz produced since the last call.
     */
    take_audio(): Float32Array;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_emulator_free: (a: number, b: number) => void;
    readonly emulator_frame: (a: number) => void;
    readonly emulator_framebuffer: (a: number) => [number, number];
    readonly emulator_new: (a: number, b: number) => number;
    readonly emulator_pause: (a: number) => void;
    readonly emulator_reset: (a: number) => void;
    readonly emulator_set_button: (a: number, b: number, c: number) => void;
    readonly emulator_take_audio: (a: number) => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
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
