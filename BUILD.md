# Building Simple EQ

## Prerequisites

### Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### System dependencies

**Linux (Debian/Ubuntu)**
```bash
sudo apt update && sudo apt install -y \
    build-essential pkg-config \
    libgl1-mesa-dev libx11-dev libxcb1-dev \
    libxcb-util-dev libxcb-icccm4-dev libxcb-image0-dev \
    libxcb-keysyms1-dev libxcb-render-util0-dev libxcb-render0-dev \
    libxcb-shape0-dev libxcb-xfixes0-dev libxcb-xkb-dev
```

**macOS** — Xcode Command Line Tools only:
```bash
xcode-select --install
```

**Windows** — Install [Build Tools for Visual Studio](https://visualstudio.microsoft.com/visual-cpp-build-tools/) (MSVC toolchain).

---

## Building

From the workspace root (`simple-eq/`):

### Release build (recommended)

```bash
cargo xtask bundle simple-eq --release
```

The bundler produces two plugins side by side:

| Format | Path |
|--------|------|
| VST3   | `target/bundled/Simple EQ.vst3` |
| CLAP   | `target/bundled/Simple EQ.clap` |

### Debug build

```bash
cargo xtask bundle simple-eq
```

Output lands in `target/bundled/` with the same filenames.

---

## Installing the VST3

Copy the `.vst3` bundle to the standard scan path for your OS:

| OS | Path |
|----|------|
| Linux | `~/.vst3/` |
| macOS | `~/Library/Audio/Plug-Ins/VST3/` |
| Windows | `C:\Program Files\Common Files\VST3\` |

**Linux**
```bash
cp -r "target/bundled/Simple EQ.vst3" ~/.vst3/
```

**macOS**
```bash
cp -r "target/bundled/Simple EQ.vst3" ~/Library/Audio/Plug-Ins/VST3/
```

**Windows (PowerShell)**
```powershell
Copy-Item -Recurse "target\bundled\Simple EQ.vst3" "$env:COMMONPROGRAMFILES\VST3\"
```

After copying, rescan plugins in your DAW.

---

## Installing the CLAP (optional)

| OS | Path |
|----|------|
| Linux | `~/.clap/` |
| macOS | `~/Library/Audio/Plug-Ins/CLAP/` |
| Windows | `C:\Program Files\Common Files\CLAP\` |

---

## Parameters

| Knob | Frequency | Slope | Range |
|------|-----------|-------|-------|
| **Lows** | 200 Hz low shelf | 6 dB/oct | −18 … +18 dB |
| **Highs** | 8 kHz high shelf | 6 dB/oct | −18 … +18 dB |

Both parameters support sample-accurate automation and use 20 ms linear smoothing to prevent zipper noise during real-time knob movement.

---

## Filter math

The plugin uses first-order IIR shelving filters derived via the bilinear transform.

**Low shelf** (transition at `fc = 200 Hz`, linear gain `A = 10^(dB/20)`):

```
K  = tan(π · fc / fs)
b0 = (K + A) / (K + 1)
b1 = (A − K) / (K + 1)
a1 = (1 − K) / (K + 1)
```

DC gain = A, Nyquist gain = 0 dB. ✓

**High shelf** (transition at `fc = 8000 Hz`):

```
K  = tan(π · fc / fs)
b0 = A · (K + 1) / (K + A)
b1 = A · (1 − K) / (K + A)
a1 = (A − K) / (K + A)
```

DC gain = 0 dB, Nyquist gain = A. ✓

Difference equation per sample: `y[n] = b0·x[n] + b1·x[n−1] − a1·y[n−1]`

---

## Troubleshooting

**`cargo xtask` not found** — run from the workspace root, not the inner `simple-eq/` crate directory.

**Link errors on Linux** — ensure all `libxcb-*` packages listed above are installed.

**Plugin not showing in DAW** — confirm the `.vst3` directory (not just the file) is in the scan path and trigger a rescan.

**Clicking when automating knobs** — both parameters have 20 ms smoothing built in; clicking at extreme rates may indicate the DAW is not honouring the plugin's latency report.
