// Runs the original hand-assembled WASM terrain kernel from index.html under Node
// and writes reference samples that the Rust `sim` crate's tests compare against.
//
// Usage: node tools/dump_terrain_reference.mjs
// Output: crates/sim/tests/fixtures/terrain_reference.txt
//
// Each output line: heightScale seedOffset x z height normalX normalY normalZ

import { writeFileSync } from "node:fs";
import { loadOriginalWasm, root } from "./load_original_wasm.mjs";

const engine = await loadOriginalWasm();

const lines = [];
const params = [[3.0, 0.0], [5.5, 0.0], [3.0, 42.0], [1.2, 7.25]];
for (const [heightScale, seed] of params) {
    engine.f32[0] = heightScale;
    engine.f32[1] = seed;
    // Grid across the whole island, including the beach falloff and the ocean trench.
    for (let x = -80; x <= 80; x += 10) {
        for (let z = -80; z <= 80; z += 10) {
            const h = engine.height(x, z);
            const n = engine.normal(x, z);
            lines.push([heightScale, seed, x, z, h, n.x, n.y, n.z].join(" "));
        }
    }
}

const outPath = `${root}/crates/sim/tests/fixtures/terrain_reference.txt`;
writeFileSync(outPath, lines.join("\n") + "\n");
console.log(`wrote ${lines.length} samples to ${outPath}`);
