// Runs the original `stepBirdFlock` WASM kernel from index.html under Node and
// writes before/after snapshots that the Rust `sim` crate's tests compare against.
//
// Usage: node tools/dump_bird_reference.mjs
// Output: crates/sim/tests/fixtures/bird_kinematics_reference.txt
//
// Each line is one bird, stepped `steps` times with a fixed `dt`:
//   dt steps
//   x y z vx vy vz flapPhase gliding(0/1)                    (inputs)
//   x y z flapPhase innerWing outerWing bodyBob              (outputs)

import { writeFileSync } from "node:fs";
import { loadOriginalWasm, root, seededRandom } from "./load_original_wasm.mjs";

const { f32, exports, constants } = await loadOriginalWasm();
const { BIRD_BASE_F32: b, BIRD_STRIDE_F32: STRIDE } = constants;

const rand = seededRandom(5678);
const range = (lo, hi) => lo + rand() * (hi - lo);

const lines = [];
for (let i = 0; i < 48; i++) {
    const dt = [1 / 30, 1 / 60, 1 / 144][i % 3];
    const steps = 1 + (i % 5) * 10;
    const gliding = i % 2;

    // Each bird runs alone in slot 0, so one never affects another.
    f32.fill(0, b, b + STRIDE);
    f32[b + 0] = range(-40, 40);
    f32[b + 1] = range(18, 46);
    f32[b + 2] = range(-40, 40);
    f32[b + 3] = range(-12, 12);
    f32[b + 4] = range(-3, 3);
    f32[b + 5] = range(-12, 12);
    f32[b + 6] = range(0, Math.PI * 2);
    f32[b + 7] = gliding;

    const inputs = Array.from(f32.slice(b, b + 8));
    for (let s = 0; s < steps; s++) exports.stepBirdFlock(1, dt, 0);
    const outputs = [f32[b], f32[b + 1], f32[b + 2], f32[b + 6], f32[b + 13], f32[b + 14], f32[b + 15]];

    lines.push([Math.fround(dt), steps, ...inputs, ...outputs].join(" "));
}

const outPath = `${root}/crates/sim/tests/fixtures/bird_kinematics_reference.txt`;
writeFileSync(outPath, lines.join("\n") + "\n");
console.log(`wrote ${lines.length} birds to ${outPath}`);
