# Roadmap

Design lives in [SPEC.md](./SPEC.md). This is the build order.

## Track A — on-device runtime (`runtime/`)

Vertical slices; each one runs on the AVD before the next starts.

- **M1 — bare pipeline.** Attach to `binder:binder_transaction`, push
  `{src_pid, dst_pid, code, flags, size}` through the ring buffer, print to
  console. Proves toolchain, cross-compile, attach, ring buffer, SELinux. *(scaffolded)*
- **M2 — process names.** Resolve `/proc/<pid>/cmdline` with a pid→name cache;
  emit `name (pid) -> name (pid)`.
- **M3 — interface descriptor.** Probe copies raw descriptor bytes out of the
  parcel head; consumer does UTF-16→UTF-8 decode → real interface name. Add
  `data_size`. (Ports the proven PoC extraction.)
- **M4 — in-kernel filter.** Hash descriptor bytes in the probe; match against a
  BPF map of wanted hashes to drop before the ring buffer.
- **M5 — errors + sinks + CLI.** Second attach point for
  `BR_FAILED_REPLY`/`BR_DEAD_REPLY` (toggleable); logcat/console/file + JSONL
  sinks; CLI args (start/stop, sink, filter, error toggle).

## Track B — offline decode

- **B1 — catalog builder** (`catalog/`, Python): folder of AIDL → JSON catalog via
  generated stubs / `aidl --dumpapi`; handle explicit `= N` ids and special
  transactions.
- **B2 — shared decoder core + `bindfetto-decode` CLI**: line parse + catalog
  lookup → method name.
- **B3 — viewer plugins**: VS Code first, DLT Viewer for the automotive audience.

## Track C — control app (`app/`, Kotlin)

- **C1 — control channel**: unix socket + command protocol (shared with the CLI).
- **C2 — app**: deploy binary (signature permission), start/stop, interface
  filter, error toggle.

Tracks B and C start once Track A produces stable output (≈after M3).
