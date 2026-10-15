// Runs the original `stepDeerKinematics` WASM kernel from index.html under Node and
// writes before/after snapshots that the Rust `sim` crate's tests compare against.
//
// Usage: node tools/dump_deer_reference.mjs
// Output: crates/sim/tests/fixtures/deer_kinematics_reference.txt
//
// Each line is one deer, stepped `steps` times with a fixed `dt`:
//   dt steps
//   x z heading speed targetSpeed walkCycle normX normY normZ        (inputs)
//   x y z speed walkCycle normX normY normZ                          (outputs)
//   hipFL kneeFL hipFR kneeFR hipBL kneeBL hipBR kneeBR              (outputs)

import { writeFileSync } from "node:fs";
import { loadOriginalWasm, root, seededRandom } from "./load_original_wasm.mjs";

const engine = await loadOriginalWasm();
const { f32, exports, constants } = engine;
const { DEER_BASE_F32: BASE, DEER_STRIDE_F32: STRIDE } = constants;

const rand = seededRandom(1234);
const range = (lo, hi) => lo + rand() * (hi - lo);

// A mix of standing, walking and trotting deer, at varied frame rates.
const DEER_COUNT = 48;
const cases = [];
for (let i = 0; i < DEER_COUNT; i++) {
    const n = [range(-0.3, 0.3), range(0.8, 1.0), range(-0.3, 0.3)];
    cases.push({
        dt: [1 / 30, 1 / 60, 1 / 144][i % 3],
        steps: 1 + (i % 5) * 10,
        x: range(-40, 40),
        z: range(-40, 40),
        heading: range(0, Math.PI * 2),
        speed: [0, 0.5, 2.15, 5.2][i % 4],
        targetSpeed: [0, 2.15, 5.2, 0][(i >> 2) % 4],
        walkCycle: range(0, Math.PI * 2),
        normal: n,
    });
}

const lines = [];
for (const c of cases) {
    // Run each deer on its own, in slot 0, so one deer never affects another.
    f32.fill(0, BASE, BASE + STRIDE);
    const b = BASE;
    f32[b + 0] = c.x;
    f32[b + 2] = c.z;
    f32[b + 3] = c.heading;
    f32[b + 5] = c.speed;
    f32[b + 6] = c.targetSpeed;
    f32[b + 7] = c.walkCycle;
    [f32[b + 12], f32[b + 13], f32[b + 14]] = c.normal;

    // Record the inputs as the f32 values the kernel actually sees.
    const inputs = [f32[b], f32[b + 2], f32[b + 3], f32[b + 5], f32[b + 6], f32[b + 7],
        f32[b + 12], f32[b + 13], f32[b + 14]];

    for (let s = 0; s < c.steps; s++) exports.stepDeerKinematics(1, c.dt, 0);

    const outputs = [f32[b], f32[b + 1], f32[b + 2], f32[b + 5], f32[b + 7],
        f32[b + 12], f32[b + 13], f32[b + 14]];
    const legs = Array.from(f32.slice(b + 15, b + 23));
    lines.push([Math.fround(c.dt), c.steps, ...inputs, ...outputs, ...legs].join(" "));
}

const outPath = `${root}/crates/sim/tests/fixtures/deer_kinematics_reference.txt`;
writeFileSync(outPath, lines.join("\n") + "\n");
console.log(`wrote ${lines.length} deer to ${outPath}`);
