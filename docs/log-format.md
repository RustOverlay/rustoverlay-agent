# `output_log.txt` — format notes

Everything here was verified against a live Rust install (Unity 6000.3.15x1-8, protocol 286) rather
than inferred. The parsers in `RustOverlay.Agent.Core.Parsing` point at this file instead of
repeating the rationale inline.

**Path:** `<Rust install>\output_log.txt`, next to the exe. Not `Player.log`, not in `LocalLow`.
`-nolog` in Steam launch options suppresses the file entirely — detect and tell the user.

## Line shape

```
2026-08-02T21:30:17.120Z|0x6e08|Initialize engine version: 6000.3.15x1-8
```

`ISO8601|threadid|message`. **Only the first line of multi-line output carries the prefix**; every
following line is bare until the next prefixed line. `LogBlockAssembler` groups them.

Unity appends a stack trace to every `Debug.Log`, so a block's body is a mix of real payload and
noise:

```
UnityEngine.DebugLogHandler:Internal_Log(LogType, LogOption, String, Object)
Client:%7add52c18fed5dbafbceafa8bcced20c138533e6(%82600b48a279eb7e76a74dd6d2411e58a5a422e4)
Client:Update()
```

Payload and stack frames are not reliably distinguishable by shape, so the assembler keeps
everything and each parser rejects what it does not recognise. `%<sha1>` entries are Facepunch's
symbol obfuscation, not corruption.

## `combatlog`

```
time    attacker  target weapon ammo area    distance old_hp new_hp info    hits integrity travel mismatch desync
344.98s polarbear you    N/A    N/A  generic 3.6m     55.9   10.9           0    0.00      0.00s  0.00m    0
338.82s polarbear you    N/A    N/A  generic 1.0m     8.0    2.0    killed  0    0.00      0.00s  0.00m    0
```

Three traps, all handled by `CombatLogParser`:

1. **`info` is blank on most rows**, so a row is 14 or 15 whitespace-separated tokens.
2. **Column widths shift between dumps** — the header itself is not byte-stable, so fixed-offset
   slicing fails. Split on whitespace.
3. **Player names may contain spaces** and the format does not quote them, so left-to-right
   tokenisation misassigns fields.

The parser therefore anchors from the right: the last five columns are fixed, the slot before them
is `new_hp` when `info` is absent (and `new_hp` is always numeric, which is the disambiguator), and
whatever sits between `time` and `weapon` is the two names. `"you"` is the reliable split point
because one side of a local combatlog row is always the local player.

**`time` is relative to the dump**, in seconds before the header line's timestamp.

### Deduplication needs a tolerance, not equality

The buffer holds ~30 events and replays them in every dump, so consecutive dumps overlap. The same
hit resolves to a *slightly different* absolute time each dump, because offsets are logged to 10ms
against different headers. Observed on the same bear hit 21 minutes apart:

| Dump header | Offset | Resolved |
|---|---|---|
| 22:26:57.337 | 344.98s | 22:21:12.357 |
| 22:48:12.936 | 1620.59s | 22:21:12.346 |

11ms apart. `CombatLogDeduplicator` matches on `(attacker, target, old_hp, new_hp, distance)` within
a 1s window.

## `printpos` / `printeyes`

**The console does not echo the command.** Only a bare parenthesised triple on its own line
appears, with no label:

```
2026-08-02T21:41:41.696Z|0x6e08|(-115.65, -1.04, 152.49)     printpos  → x, y, z
2026-08-02T21:42:20.971Z|0x6e08|(8.02, 170.41, 0.00)         printeyes → pitch, yaw, roll
```

This is why grepping the log for `printpos` finds only `find` help text and concludes, wrongly, that
the commands were never run.

- Rust is **Y-up**; north is **+Z**.
- Euler angles are **0–360 and positive pitch is downward** — looking up 8° reads `351.98`.
- **Roll is 0.00 outside vehicles.**
- Yaw is *believed* to map 1:1 to compass bearing. Still UNVERIFIED — confirm against an equipped
  compass.

Position and orientation are indistinguishable from the line alone, so `TransformCorrelator`
assigns meaning by emission order — the agent owns the keybind, so it knows the order. Order alone
is not sufficient when a user types a command by hand, so a candidate is rejected as an orientation
unless all three components fall in `[0, 360)`. World coordinates go negative and exceed 360 on any
map above 720m, which makes the guard decisive in practice.

### Binds are samples, not a stream

`cfg/keys.cfg`, commands chain with `;`. Binds fire on key **down** and **up** and do **not** repeat
while held, so a held key is one sample. Good targets: `mouse1` (per-shot 6DoF), reload, jump. `W`
floods the log with combatlog rows.

## Identity and player state

```
2026-08-02T21:30:39.194Z|0x6e08|SteamID: 76561190000000001 (testplayer)
```

There is also an `Entity: player` dump — name, SteamID and ~30 boolean state flags including
`Wounded`, `Sleeping`, `Incapacitated`, `SafeZone`, `Aiming`, `Connected`. Not documented in the
upstream research; parsed by `PlayerParser.TryParsePlayerState`.

> `steam.use_steam_nicknames` in `cfg/client.cfg` makes displayed names local-only aliases, so a
> name read from the log may not match the Steam profile name. Downgrade confidence when set.
