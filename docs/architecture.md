# Architecture

A Cargo workspace; `ro-app` is the agent binary.

| Crate | Owns | Depends on |
|---|---|---|
| `ro-vision` | HUD geometry and readers, the item catalogue, icon signatures and the matcher, `LoadoutState` | — |
| `ro-core` | log tailing and parsing, game events, settings, install discovery, key tables, capture bookkeeping, the hosting state services share | `ro-vision` |
| `ro-net` | Steam sign-in, the teams API, the relay uplink, `proto/` codegen, the DPAPI token store | `ro-core`, `ro-vision` |
| `ro-capture` | screen capture (DXGI duplication, GDI fallback), the GDI+ icon downscale and glyph atlas, the vision service | `ro-core`, `ro-vision` |
| `ro-audio` | gunshot audio capture and its replay tool | — |
| `ro-app` | the egui main window and overlay, Win32 interop, the background services | all |
| `belt-corpus`, `belt-recorder` | matcher calibration tools, not shipped; see [matcher-tools.md](matcher-tools.md) | `ro-core`, `ro-vision`, `ro-capture` |
| `calib-recorder` | gunshot calibration recorder, not shipped | `ro-core`, `ro-audio` |

`ro-core` and `ro-vision` are platform-neutral except where Windows is inherent (the registry Steam
lookup), which is gated with `#[cfg(windows)]`.

## Matcher changes are judged by the corpus

The agent began as a bit-exact port of a C# version, which has since been deleted; nothing has to
match it any more. A change to the matcher, the icon pipeline or `SlotHistory` — including replacing
GDI+ — is in if `crates/belt-corpus/score/run.sh` shows it reading more slots right without reading
more of them wrong, and its new baseline is committed with it. `TODO(port):` comments mark places
where the port kept a C# quirk on purpose; they are candidates, not obligations.

## Conventions

- No file over 300 lines; one module per concern.
- Traits at seams (`FrameSource`, `TokenStore`, …), plain constructor arguments, `tracing` for logs,
  `serde` structs for settings, `tokio` for async.
- Comments only for non-obvious logic, gotchas and TODOs, one line each.
- Each crate's `Cargo.toml` uses `edition.workspace = true` and `[lints] workspace = true`.
- `cargo build` and `cargo test` exceed a minute cold; run them in the background, and filter with
  `cargo test -p <crate>`.
