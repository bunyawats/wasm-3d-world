// Loads PART 1 of index.html (the hand-assembled WASM engine) under Node, so the
// reference dumpers can run the original kernels exactly as the browser does.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

export const root = fileURLToPath(new URL("..", import.meta.url));

/**
 * Returns `{ exports, f32, height, normal, constants }` for the original engine.
 * `f32` is the Float32Array view of its shared linear memory.
 */
export async function loadOriginalWasm() {
    const html = readFileSync(`${root}/index.html`, "utf8");

    // Slice out PART 1 of the main script, unchanged.
    const start = html.indexOf("let wasmMemory;");
    const end = html.indexOf("// PART 2:");
    if (start < 0 || end < 0) throw new Error("PART 1 markers not found in index.html");
    const part1 = html.slice(start, html.lastIndexOf("// ====", end));

    // Minimal stubs for the browser globals PART 1 touches.
    const context = {
        WebAssembly,
        Math,
        Float32Array,
        Uint8Array,
        ArrayBuffer,
        DataView,
        TextEncoder,
        document: { getElementById: () => ({}) },
        THREE: { Vector3: class { constructor(x, y, z) { Object.assign(this, { x, y, z }); } } },
    };
    vm.createContext(context);
    vm.runInContext(
        `${part1}
         globalThis.engine = async () => {
             await initWasmEngine();
             return {
                 exports: wasmInstance.exports,
                 f32: wasmF32,
                 height: getTerrainHeightWasm,
                 normal: getTerrainNormalWasm,
                 constants: { DEER_BASE_F32, DEER_STRIDE_F32, BIRD_BASE_F32, BIRD_STRIDE_F32 },
             };
         };`,
        context,
    );
    return context.engine();
}

/** Small deterministic PRNG (mulberry32), so fixtures are reproducible. */
export function seededRandom(seed) {
    let a = seed >>> 0;
    return () => {
        a = (a + 0x6d2b79f5) >>> 0;
        let t = a;
        t = Math.imul(t ^ (t >>> 15), t | 1);
        t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
}
