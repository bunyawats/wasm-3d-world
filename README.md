# 🌍 WASM 3D Low-Poly World & Wildlife Simulation

**[🚀 Live Demo: Launch the 3D World](https://bunyawats.github.io/wasm-3d-world/)**

An interactive, procedural 3D low-poly island ecosystem built with **Three.js** and powered by an in-browser compiled **WebAssembly (`WebAssembly.Module` & `WebAssembly.Instance`)** physics, terrain heightfield, and biomechanical kinematics engine.

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

---

## 🛠️ Tech Stack

- **WebAssembly (Raw Binary / LEB128 Bytecode Assembler)** — Terrain height/normal computation & creature joint kinematics
- **Three.js (`v0.160.0`)** — 3D WebGL rendering, lighting, shadows, and skeletal hierarchy
- **Tailwind CSS** — Glassmorphism UI & responsive telemetry HUD
