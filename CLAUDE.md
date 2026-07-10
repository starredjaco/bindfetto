# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Bindfetto observes Android **Binder** IPC at the kernel level and surfaces it as
human-readable transaction logs. Design is in `SPEC.md`; build order and current
milestone status are in `ROADMAP.md` (read it first when resuming work).

The system splits into two halves by design:

- **Runtime capture path** (`runtime/`, on-device, fast): an eBPF probe pushes a
  compact record per transaction through a ring buffer; a Rust userspace consumer
  drains, resolves process names, and emits. It emits the **raw** transaction code —
  no method-name lookup on the hot path.
- **Offline decode path** (`decode/` + `catalog/` + `plugins/`): resolves raw codes
  to method names *after the fact* against a precompiled AIDL catalog, so the same
  captured logs can be re-decoded against any catalog version.

A third piece, the **control app** (`app/`, Kotlin/Compose), drives the runtime live
over a TCP control channel.

## Architecture

### Runtime (`runtime/` — Cargo workspace, Rust)

Three crates, one wire contract between them:

- `bindfetto-common` — the `#[repr(C)]` `TxEvent`: the ring-buffer wire contract
  shared by probe and consumer. `no_std` by default; the `user` feature pulls in
  `aya` to impl `Pod` for zero-copy reads userspace-side.
- `bindfetto-ebpf` — the `no_std` eBPF probe, built for `bpfel-unknown-none`. NOT a
  default workspace member; the consumer's `build.rs` compiles it via `aya-build` and
  embeds the object so the binary loads it at runtime.
- `bindfetto` — the userspace consumer. Loads the probe, drains the ring buffer,
  resolves pid→name from `/proc/<pid>/cmdline` (cached), and emits. `src/main.rs`
  holds the sinks (`Sink` enum, `logcat`/`dlt` modules), `RuntimeState`, and the
  `control` module (line-oriented TCP server). `src/dlt_wire.rs` is the DLT message
  encoder.

Capture is gated **in-kernel** by BPF maps for cheapness: a `WANTED` map keyed by the
zero-padded UTF-16LE interface descriptor (collision-free exact match), a 1-element
`FILTER_ON` flag, and an `ERRORS_ON` flag. Non-matching transactions drop in the
tracepoint *before* the ring buffer. Errors come from a second `binder:binder_return`
attach point correlated per-thread to the failing transaction (`LAST_TX` map), with
the concrete errno recovered from the kernel's `failed_transaction_log` by `debug_id`.

`aya` is Linux-only, so the consumer only builds for the Android target, not the
macOS host.

### Decode core (`decode/` — Rust, host-built)

The plugin-agnostic core the CLI and both viewer plugins are thin adapters over.
`Decoder`/`Catalog` do `(interface, code) → method`; `Decoder::decode_line` rewrites
`interface.[code:N]` tokens in a line in place (prefix-agnostic — works with console
timestamps, the `BINDFETTO` marker, or logcat/DLT wrapping). Exposed three ways:

- `bindfetto-decode` CLI (`main.rs`) — stdin→stdout / file.
- C ABI (`ffi.rs` + `include/bindfetto_decode.h`, `staticlib`/`cdylib`) — for the DLT
  Viewer plugin and native embedders.
- WASM (`wasm32-unknown-unknown`, re-exported from `plugins/vscode/wasm/`) — for the
  VS Code extension.

Kept separate from `runtime/` because it builds on the host.

### Catalog builder (`catalog/` — Python 3, stdlib only)

`bindfetto_catalog.py` turns AIDL (file, recursed folder, or http(s) URL) into the
`interface → { code → method }` JSON catalog. Methods numbered from
`FIRST_CALL_TRANSACTION` (1) in declaration order unless a trailing `= N` fixes the
code. `const`s and nested types don't consume codes. **Codes are aligned to the exact
AIDL you feed it — use AIDL matching the device build.**

### Plugins (`plugins/`)

- `plugins/dlt/` — DLT Viewer decoder plugin (C++/Qt `QDLTPluginDecoderInterface`
  over the core's C ABI). Recognizes bindfetto lines by the `BINDFETTO` marker.
- `plugins/vscode/` — VS Code extension (TypeScript over the WASM core).

### Control app (`app/` — Kotlin + Jetpack Compose)

An ordinary app can't grant itself root/BPF, so the runtime runs as a **root daemon**
(started via adb) and the app controls it over TCP (default `127.0.0.1:3491`).
Start/Stop toggles capture on that daemon — it does not spawn the process. Client
logic is a thin wrapper over the line protocol in
`app/app/src/main/java/com/bindfetto/control/ControlClient.kt`. Three tabs: Control,
Filter (interface discovery only while the tab is open), Deploy (best-effort `su`
deploy with adb fallback).

## Data flow (end to end)

```
eBPF probe → ring buffer → consumer (name resolve, raw code) → sink(s)
                                                                  │
        console / logcat / JSONL / DLT-serve ─────────────────────┘
                                                                  │
                    offline: catalog (AIDL→JSON) + decode core ───┘ → method names
```

The hot path never decodes method names; that is always offline against a catalog.

## Build & test

### Runtime (from `runtime/`)

Needs nightly Rust (pinned in `rust-toolchain.toml`) + `rust-src`, `bpf-linker`, the
`aarch64-linux-android` target, and the Android NDK (linker wired in
`.cargo/config.toml`, expects `aarch64-linux-android30-clang` on PATH). `aya` is
Linux-only — the consumer only cross-compiles to Android, it does not build on macOS.

```sh
cargo build --release --target aarch64-linux-android   # embeds the eBPF object via build.rs
adb push target/aarch64-linux-android/release/bindfetto /data/local/tmp/
adb shell /data/local/tmp/bindfetto                     # run as root
```

On an **arm64** AVD, eBPF load needs root + permissive SELinux:

```sh
adb root && adb shell setenforce 0
# Tracepoint field offsets in bindfetto-ebpf/src/main.rs must match this device:
adb shell cat /sys/kernel/tracing/events/binder/binder_transaction/format
```

Key CLI flags: `--sink console|logcat|both|none`, `--jsonl <path>`,
`--dlt-serve [port]` (default 3490), `--iface <name>` (repeatable/comma-separated
in-kernel filter, exact match), `--errors [on|off]`, `--include-replies`,
`--control [port]` (default 3491; auto-binds the DLT server).

### Decode core (from `decode/`) — builds on host

```sh
cargo build --release   # produces libbindfetto_decode.a + C header for the DLT plugin
cargo test
```

### Catalog builder (from `catalog/`)

```sh
python3 bindfetto_catalog.py -o catalog.json /path/to/aosp/frameworks/base
python3 -m unittest discover -s tests -v
```

### VS Code extension (from `plugins/vscode/`)

```sh
rustup target add wasm32-unknown-unknown
npm install && npm run build:wasm && npm run compile
npm run smoke   # standalone Node check of the wasm decoder, no VS Code needed
```

### DLT plugin (from `plugins/dlt/`)

Native Qt shared library — must match your dlt-viewer's Qt major + compiler ABI, and
needs the dlt-viewer `qdlt` SDK. Build the decode core first, then:

```sh
cmake -B build -DDLT_VIEWER_QDLT_INCLUDE_DIR=/path/to/dlt-viewer/qdlt \
               -DDLT_VIEWER_QDLT_LIB=/path/to/dlt-viewer/build/lib/libqdlt.so
cmake --build build
```

### Control app (from `app/`)

Needs JDK 17+. Building the **runtime** first bundles the binary into the app's
`jniLibs` (a Gradle task copies it) for the Deploy tab; otherwise Deploy just shows
the adb fallback.

```sh
export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
./gradlew :app:assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

## Conventions worth knowing

- The runtime hot path stays cheap on purpose: **never add method-name resolution or
  other catalog work to the on-device path** — that belongs in the offline decode
  core, so logs stay re-decodable against any catalog.
- Interface filtering and error capture are toggled through **BPF flag maps**, so they
  can be flipped live over the control channel without reattaching.
- The decode core is written **once** and reused via three ABIs (CLI rlib, C
  staticlib/cdylib, WASM). Decode logic changes go in `decode/`, not in a plugin.
- `TxEvent` in `bindfetto-common` is the probe↔consumer wire contract — changing it
  means changing both sides.
