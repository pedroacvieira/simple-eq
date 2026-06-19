# Simple EQ

A minimal two-band shelving equaliser built in Rust as a VST3/CLAP audio plugin.

- **Lows** — first-order low shelf at 200 Hz, ±18 dB
- **Highs** — first-order high shelf at 8 kHz, ±18 dB

Both bands use 6 dB/octave slopes (first-order IIR filters via bilinear transform) and support sample-accurate automation with 20 ms parameter smoothing.

Built with [nih-plug](https://github.com/robbert-vdh/nih-plug). Supports stereo and mono signal paths.

---

## Prerequisites

### Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### System libraries

**Linux (Debian/Ubuntu)**
```bash
sudo apt update && sudo apt install -y \
    build-essential pkg-config \
    libgl1-mesa-dev libx11-dev libxcb1-dev \
    libxcb-util-dev libxcb-icccm4-dev libxcb-image0-dev \
    libxcb-keysyms1-dev libxcb-render-util0-dev libxcb-render0-dev \
    libxcb-shape0-dev libxcb-xfixes0-dev libxcb-xkb-dev
```

**macOS** — Xcode Command Line Tools:
```bash
xcode-select --install
```

**Windows** — [Build Tools for Visual Studio](https://visualstudio.microsoft.com/visual-cpp-build-tools/) (MSVC toolchain).

---

## Build

All commands run from the workspace root (`simple-eq/`).

```bash
# Release build (recommended)
cargo xtask bundle simple-eq --release

# Debug build
cargo xtask bundle simple-eq
```

Output is written to `target/bundled/`:

| Format | File |
|--------|------|
| VST3 | `target/bundled/Simple EQ.vst3` |
| CLAP | `target/bundled/Simple EQ.clap` |

---

## Install

Copy the bundle to your OS's standard plugin directory, then rescan in your DAW.

**Linux**
```bash
cp -r "target/bundled/Simple EQ.vst3" ~/.vst3/
# CLAP
cp -r "target/bundled/Simple EQ.clap" ~/.clap/
```

**macOS**
```bash
cp -r "target/bundled/Simple EQ.vst3" ~/Library/Audio/Plug-Ins/VST3/
# CLAP
cp -r "target/bundled/Simple EQ.clap" ~/Library/Audio/Plug-Ins/CLAP/
```

**Windows (PowerShell)**
```powershell
Copy-Item -Recurse "target\bundled\Simple EQ.vst3" "$env:COMMONPROGRAMFILES\VST3\"
```

---

## Test

Unit tests cover coefficient correctness, filter direction, steady-state response, state management, and stability — no DAW or audio hardware needed.

```bash
cargo test -p simple-eq
```

To test the loaded plugin binary against the VST3 spec:

```bash
# pluginval (https://github.com/Tracktion/pluginval)
pluginval --validate-in-process "target/bundled/Simple EQ.vst3"
```

---

## Project structure

```
simple-eq/
├── simple-eq/
│   └── src/lib.rs      plugin implementation + unit tests
├── xtask/
│   └── src/main.rs     cargo xtask bundler entry point
└── BUILD.md            detailed build, install, and filter math reference
```

---

## License

MIT
