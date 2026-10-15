# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

An interactive low-poly 3D island with deer and bird wildlife, rendered with Three.js. Its physics and kinematics run in a WebAssembly module that is **assembled byte-by-byte in JavaScript at page load**. The whole app lives in a single file, `index.html`. It is served via GitHub Pages at https://bunyawats.github.io/wasm-3d-world/.

## Rust rewrite (in progress)

A port to Rust + Bevy is under way in a Cargo workspace. `index.html` stays as the reference implementation until the Rust version reaches parity. The owner is learning Rust, so Rust code is written with explanatory comments aimed at a learner. Keep that style.

- `crates/sim`: engine-independent simulation, built and tested natively. Its only dependency is `glam`, kept at the version Bevy uses, so `Vec3` is shared with the app. It covers terrain, deer (kinematics plus pose), the herd, birds (`bird.rs`: kinematics, steering, `Flocks`), the day/night palette and `Clock` (`daylight.rs`), scenery placement and deer obstacles (`scenery.rs`), and a seeded `Rng` that replaces `Math.random()`. Parity-ported kernels mirror the original's floating-point operation order and do trig in `f64` (like JS `Math`) to stay bit-identical. Don't "simplify" those expressions or swap in `std` constants such as `PI`.
- `crates/app`: Bevy 0.20 frontend. Each module is one plugin (`terrain`, `water`, `environment`, `camera`) registered in `main.rs`. `IslandTerrain` is a newtype `Resource` wrapping `sim::Terrain`. It's built for the browser with Trunk from `crates/app/index.html`, which is separate from the original root `index.html`.
- Bevy's API changes between releases, so check names against the source in `~/.cargo/registry/src/*/bevy_*-0.20.0/` (examples in `bevy-0.20.0/examples/`) instead of relying on memory.
- The orbit camera computes drag from `Window::cursor_position()` deltas, because `AccumulatedMouseMotion` didn't deliver drags in the browser. Synthetic JS `PointerEvent`s don't reach winit at all, so test input with real (trusted) browser events.
- `tools/dump_*_reference.mjs` run the original WASM kernels from `index.html` under Node (shared loader: `tools/load_original_wasm.mjs`) and write fixtures to `crates/sim/tests/fixtures/`. The `crates/sim/tests/*_parity.rs` tests check the Rust ports against them. Terrain, deer and bird kinematics currently match bit for bit.
- `IslandTerrain` and `IslandScenery` are created in `TerrainPlugin::build`, not in a `Startup` system, because other plugins' startup systems (the deer obstacles) need them and Bevy doesn't order startup systems across plugins. The night light is a 'moon' mirrored above the horizon; the original lit the island from below at night.
- `crates/app/src/rig.rs` holds the shared `Rig` helper for building creatures from child entities. `offset_part` reproduces a Three.js geometry that was `translate()`d before its mesh was rotated, which takes two transform levels in Bevy. Birds (`birds.rs`) follow the same pattern as deer. Bird rigs are created lazily by `spawn_missing_rigs` for any `sim` bird without one, so the B key can add flocks at runtime. The rig faces +Z, so it's aimed with `looking_to(-velocity)`.
- In `crates/app/src/deer.rs`, each deer is an entity tree: a root with `DeerId` and a `DeerRig` that holds its joint `Entity`s, joined by `ChildOf`. Each frame, `sync_rigs` copies `sim::Herd` state onto the joints. Deer decisions (behaviour state machine, target picking, steering, startle) live in `crates/sim/src/brain.rs` as a separate `impl Deer` block. They use the RNG, so they're covered by behavioural tests in `herd.rs` instead of parity fixtures. Deer occasionally wade into the inland bay's shallows; the original does this too, because its water-escape rule turns toward the island centre.

Commands:
- `cargo test` (single test: `cargo test -p sim --test terrain_parity height_matches`)
- `cargo fmt && cargo clippy --all-targets -- -D warnings`
- `node tools/dump_{terrain,deer,bird}_reference.mjs` regenerate the parity fixtures
- Keys in the app: S startles the herd, B adds a bird flock, T starts or stops the day/night clock, and `[` / `]` step the hour back or forward. On-screen controls come with the HUD (milestone 7).
- Browser checks: Bevy only steps while the tab is visible. If `document.visibilityState` is `hidden` (for example, the automation window is behind other windows), `requestAnimationFrame` never fires and the scene looks frozen even though nothing is wrong.
- `cargo run -p app` runs the Bevy app natively
- `cd crates/app && trunk serve --port 8080` builds to WASM and serves at http://127.0.0.1:8080 with live rebuild. The first build takes about 4 minutes; rebuilds take seconds. `crates/app/Trunk.toml` adds `../sim` to the watch list; without it, edits to `sim` don't trigger a rebuild, and Trunk needs a restart to pick up config changes.

## Running

Open `index.html` in a browser, or serve the directory statically (e.g. `python3 -m http.server`) and visit `http://localhost:8000/`. All dependencies load from CDNs, so you need network access:
- Tailwind CSS Play CDN (`cdn.tailwindcss.com`), with a small inline `tailwind.config`
- Three.js **r128** (global `THREE`, not ES modules) plus `OrbitControls` from `three@0.128.0/examples/js`. The README says v0.160.0, but the code uses r128. Use only r128-compatible APIs.
- FontAwesome 6.4.0 and Google Fonts

To verify a change, load the page and check the browser console. A malformed WASM byte stream fails at `WebAssembly.compile` with a validation error.

## Architecture (`index.html`)

The `<script>` block is split into commented `PART` sections. Startup runs on `window.load`: `initWasmEngine()` → `initSharedMaterials()` → `initThree()` → `setupEventListeners()` → `animate()`.

### Part 1: hand-written WASM assembler
- Helpers `uleb128`, `sleb128`, `f32Bytes`, `encodeString`, and `createSection` encode the binary format. Opcode constants (`OP_*`, `TYPE_*`) are local to `initWasmEngine`.
- The module imports `env.memory` (a 2-page / 128 KB `WebAssembly.Memory`, initial = max = 2) and `env.sin`, `env.cos`, and `env.exp` (function indices 0–2).
- Four defined functions (indices 3–6) are exported: `terrainHeight(x,z)->f32`, `computeNormal(x,z)` (writes `f32[4..6]`), `stepDeerKinematics(count,dt,time)`, and `stepBirdFlock(count,dt,time)`. Their bodies are the arrays `func3Body`…`func6Body`. Each starts with its local declarations, and the comments name each local index.
- **When editing WASM bodies:** keep the type, function, export, and code sections consistent (counts, type indices, function indices). Encode immediates correctly (`f32Bytes` for f32 constants; LEB128 for i32 constants and memarg offsets). Balance every `block`/`loop`/`if` with `OP_END`, plus the final function `OP_END`. Section and body lengths are computed automatically.

### Shared linear memory contract
JS reads and writes the same memory through `wasmF32` (a `Float32Array` view). The layout is documented in the big comment at the top of Part 1. It must stay in sync among the WASM byte offsets (byte = f32 index × 4), the JS constants, and every `wasmF32[b + N]` access:
- `f32[0..8]`: globals (heightScale, seedOffset, waterLevel, worldSize, normal scratch output, herd center)
- Deer: `DEER_BASE_F32 = 64`, stride 24 floats, max 64 deer
- Birds: `BIRD_BASE_F32 = 1600`, stride 16 floats, max 64 birds

Each creature's `userData.f32Base` is its slot's starting index. Field meanings come from the per-stride offsets (e.g. deer `+8` = state, `+15..+22` = leg joint angles).

### Parts 2–3: scene and per-frame loop
- Scene setup, terrain built from WASM heights and normals, water, flora, rocks, crystals, and the articulated deer and bird rigs, made of nested `THREE.Group` pivots stored in `userData.joints`.
- `stepSimulationWasmAndSync(delta, t)` splits the work between JS and WASM:
  1. JS runs the high-level AI (state machine and timers, steering targets, bird banking) by writing into WASM memory.
  2. JS calls the WASM kinematics kernels.
  3. JS copies joint angles and positions from memory onto the Three.js scene graph.
- `animate()` also animates the water vertices in JS, refreshes the memory-inspector table about every 120 ms, and updates the follow camera.

### UI
A Tailwind HUD overlay has a control sidebar, a creature telemetry card, and a WASM inspector drawer (exports, live memory table, hex dump of `wasmBinaryBytes`). Controls are wired by element ID in `setupEventListeners()`.
