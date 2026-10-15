# 🌍 WASM 3D Low-Poly World & Wildlife Simulation

**[🚀 Live Demo: Launch the 3D World](https://bunyawats.github.io/wasm-3d-world/)** · **[🦀 Rust + Bevy version](https://bunyawats.github.io/wasm-3d-world/rust/)**

An interactive, procedural 3D low-poly island ecosystem, in two implementations:

- **The original** (`index.html`): **Three.js**, powered by a **WebAssembly** physics, terrain and kinematics engine that is hand-assembled byte by byte and compiled in the browser (`WebAssembly.Module` & `WebAssembly.Instance`).
- **The Rust port** (`crates/`): the same world rewritten in **Rust** with the **Bevy** game engine and compiled to WebAssembly. Its simulation matches the original's WASM kernels **bit for bit**.

---

## ✨ Key Features

- **In-Browser WebAssembly Engine:** Dynamically assembles and compiles `.wasm` bytecode (`00 61 73 6d`) on startup with a shared 128 KB `WebAssembly.Memory` buffer (`Float32Array`) read directly by Three.js every frame.
- **Biomechanical Articulated Wildlife:**
  - **Roaming Deer Herd:** Multi-jointed quadruped skeletal rigs (hips, knees/hocks, neck, ears, antlers) with diagonal stride kinematics, terrain surface-normal slope alignment, and herd AI (grazing, scanning, walking, and startling/trotting).
  - **Soaring Bird Flocks:** Two-stage articulated wing rigs (inner wing arm + outer primary feathers) that fold on the upstroke, extend on the downstroke, and hold a dihedral V-posture during thermal glides.
- **Dynamic Environment:** Procedural island heightfield, animated water surface, day/night atmospheric cycle, and interactive object spawning (glowing crystals & neon trees).
- **Live WASM Memory & Bytecode Inspector:** Built-in telemetry HUD showing microsecond (`μs`) WASM kernel execution time, compiled `.wasm` binary hex dump, exported symbols, and live linear memory struct values.

---

## 🎮 Controls

- **Orbit / Rotate:** Left-click + Drag
- **Pan Camera:** Right-click + Drag
- **Zoom:** Scroll Wheel / Pinch
- **Inspect Creature:** Click directly on any deer or bird in the 3D world (or use the **Follow Deer** / **Follow Bird** buttons) to lock the tracking camera onto it.
- **Inspect WASM Engine:** Click **Inspect WASM Memory** in the top bar to view live linear memory buffers and bytecode.

The Rust version adds keyboard shortcuts: **S** startles the herd, **B** adds a bird flock, **T** starts or stops the day/night cycle, and **[** / **]** step the time by an hour. Its **Inspector** shows the live simulation state of every creature.

---

## 🛠️ Tech Stack

**Original**

- **WebAssembly (Raw Binary / LEB128 Bytecode Assembler)** — Terrain height/normal computation & creature joint kinematics
- **Three.js (r128)** — 3D WebGL rendering, lighting, shadows, and skeletal hierarchy
- **Tailwind CSS** — Glassmorphism UI & responsive telemetry HUD

**Rust port**

- **Rust** compiled to `wasm32-unknown-unknown`
- **Bevy 0.20** — rendering (WebGL2), entity-component-system, scene hierarchy for the creature rigs
- **egui** via `bevy_egui` — the HUD
- **Trunk** — builds the WebAssembly bundle and serves it locally

---

## 🦀 The Rust Port

```
crates/sim   Engine-independent simulation: terrain, deer, birds, day/night, scenery.
             Plain Rust (only depends on `glam`), tested natively.
crates/app   The Bevy app: rendering, camera, HUD. Reads state from `sim` and draws it.
tools/       Node scripts that run the original's WASM kernels to produce the
             reference data for the parity tests.
```

### Run it locally

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked

cd crates/app
trunk serve            # http://127.0.0.1:8080, rebuilds on save
```

Or natively, in a desktop window: `cargo run -p app`.

### Test

```sh
cargo test                                   # all tests
cargo test -p sim --test deer_parity         # one test file
cargo fmt && cargo clippy --all-targets -- -D warnings
```

The `*_parity` tests check the Rust terrain, deer and bird kinematics against reference output from the original hand-assembled WebAssembly. They match exactly. To regenerate that data: `node tools/dump_terrain_reference.mjs` (likewise `dump_deer_…` and `dump_bird_…`).

### Release build and deployment

```sh
cd crates/app
trunk build --release --cargo-profile wasm-release
```

Pushing to `main` runs `.github/workflows/deploy.yml`, which tests, builds and deploys both versions to GitHub Pages. In the repository settings, **Pages → Source** must be set to **GitHub Actions**.
