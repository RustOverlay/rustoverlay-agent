# Development

```
cargo build --release
cargo test
cargo run --release -p ro-app
```

Requires Windows and the MSVC build tools; `rust-toolchain.toml` pins the Rust version, which rustup
installs on first build. `scripts/check.sh` runs build, tests and clippy the way CI does.

A plain clone builds — there is no sibling checkout to arrange and no submodule to initialise,
because `proto/` is vendored. That is deliberate: "read the source and see what we send" is not a
real offer if reproducing the build needs anything you have not been given.

`crates/ro-core/tests/real_log_smoke.rs` runs the pipeline over whatever Rust install is on the
machine and prints a summary. It passes silently when Rust is absent, so it is safe in CI — its job
is to catch log-format drift after a game update, which fixtures cannot do.

Two calibration tools sit beside the agent: `belt-recorder` captures HUD samples from a live session
and `belt-corpus` scores the icon matcher against them. See [matcher-tools.md](matcher-tools.md).

## Further reading

| | |
|---|---|
| [architecture.md](architecture.md) | the crates and what each owns |
| [log-format.md](log-format.md) | `output_log.txt` and the traps in it |
| [capture-loop.md](capture-loop.md) | how the HUD is read off the screen |
| [hud-layout.md](hud-layout.md) | where the slots and vitals are |
| [item-matching.md](item-matching.md) | resolving a slot to an item |
