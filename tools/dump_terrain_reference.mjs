// Runs the original hand-assembled WASM terrain kernel from index.html under Node
// and writes reference samples that the Rust `sim` crate's tests compare against.
//
// Usage: node tools/dump_terrain_reference.mjs
// Output: crates/sim/tests/fixtures/terrain_reference.txt
//
// Each output line: heightScale seedOffset x z height normalX normalY normalZ

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const root = fileURLToPath(new URL("..", import.meta.url));
const html = readFileSync(`${root}/index.html`, "utf8");

// Slice out PART 1 (the WASM assembler) of the main script, unchanged.
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
     globalThis.init = initWasmEngine;
     globalThis.height = getTerrainHeightWasm;
     globalThis.normal = getTerrainNormalWasm;
     globalThis.setParams = (heightScale, seed) => { wasmF32[0] = heightScale; wasmF32[1] = seed; };`,
    context,
);
await context.init();

const lines = [];
const params = [[3.0, 0.0], [5.5, 0.0], [3.0, 42.0], [1.2, 7.25]];
for (const [heightScale, seed] of params) {
    context.setParams(heightScale, seed);
    // Grid across the whole island, including the beach falloff and the ocean trench.
    for (let x = -80; x <= 80; x += 10) {
        for (let z = -80; z <= 80; z += 10) {
            const h = context.height(x, z);
            const n = context.normal(x, z);
            lines.push([heightScale, seed, x, z, h, n.x, n.y, n.z].join(" "));
        }
    }
}

const outPath = `${root}/crates/sim/tests/fixtures/terrain_reference.txt`;
writeFileSync(outPath, lines.join("\n") + "\n");
console.log(`wrote ${lines.length} samples to ${outPath}`);
