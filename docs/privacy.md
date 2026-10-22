# Privacy and safety

**This repository is public so you can verify what the agent does.** Everything it reads is listed
below, and everything it sends is defined by [`proto/`](../proto/), vendored here in full so you can
read the wire format in the same clone as the code that fills it in. CI keeps it byte-identical to
the canonical [wire contract](https://github.com/RustOverlay/rustoverlay-contracts). The relay only
ever receives what this code chooses to send.

## What it reads

| Source | What for |
|---|---|
| `<Rust>\output_log.txt` | combat events, and your position/orientation when you press a bound key |
| `<Rust>\cfg\client.cfg`, `<Rust>\cfg\keys.cfg` | display mode, UI scale and your inventory key, to calibrate the overlay |
| `<Rust>\Bundles\items` | the game's own item definitions and icons, to recognise what is in a slot |
| `<Rust>\Bundles\textures` | the icons of approved skins, so skinned items are recognised too |
| Steam's workshop cache for Rust | skin icons the game has downloaded, so skinned items are recognised too |
| Steam's item schema (`<Steam>\userdata\<account>\inventorymsgcache`) | the list of approved skins and which item each repaints; only the schema file is opened, never the inventory list beside it |
| The screen, while Rust is the window in front | your belt, inventory and vitals; never anything while you are in another window |
| Rust's game audio | gunshots, to place them; nothing else is captured |
| Windows registry (`SOFTWARE\Valve\Steam`) | locating your Rust install |

Nothing is downloaded. Everything above is on your machine already; the only connections the agent
makes are sign-in, the teams API and the relay.

## What it does not do

- **No injection.** Nothing is loaded into the game process. The agent reads a log file the game
  writes on its own, using commands any player can run at auth level 0.
- **No memory reading, no packet interception, no modification of the game.**
- **No keyboard hook.** Only the keys you bind are observed — see
  [configuration.md](configuration.md#how-keys-are-read).
- **No credentials.** The agent holds no Steam password and no Rust+ token. It authenticates to the
  relay with a per-install token that grants access to nothing else.
- **No data about players outside your team.**

## Verifying a download

The installer is not code-signed yet, so Windows SmartScreen warns on first run. Every release is
built by GitHub Actions from this repository, and you can prove a download came from that build:

```
gh attestation verify RustOverlay-win-Setup.exe -R RustOverlay/rustoverlay-agent
```
