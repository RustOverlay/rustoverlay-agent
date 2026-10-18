# ro-core review (2026-10-17)

Scope: `crates/ro-core/src/**` against `src/RustOverlay.Agent.Core`. Read-only review; nothing built or run.
"Inherited" means the C# had the same defect. Parity rules keep the behaviour, but the defect is still real.

## Findings (ranked)

### 1. MEDIUM — A relay disconnect while its queue is full faults the log tail and drops log lines (inherited)
- `hosting/event_queue.rs:56-57`, `hosting/game_event_bus.rs:63-70`, `hosting/log_tail.rs:69`
- Scenario: the relay socket stalls, so `send_combat` stops draining. The Lossless "relay" queue
  (`COMBAT_QUEUE_CAPACITY` = 512, and it takes *every* event type, not only combat) fills up, and
  `publish` waits in `EventQueue::write`. When the connection then errors, `send_combat` drops its
  `Subscription`. `dispose` completes the queue, the waiting write returns `Err(ChannelClosed)`, and
  `tail_session` exits with `TailError::Closed`. Status goes to Faulted. After `RETRY_DELAY` a new
  `LogTailer` starts at EOF (`from_start=false`), so every line written during the stall plus 5 s is
  never parsed. That includes the combat dumps that Lossless mode exists to protect.
- Fix: a subscriber that detaches should not fail the publisher. Give `EventQueue` a `disposed` flag
  (separate from `completed`), or have `publish` check whether the subscriber is still in the bus.
  Then treat a closed queue that is no longer subscribed as `Ok(())`, and return `ChannelClosed` only
  for a queue that was completed while still subscribed.

### 2. MEDIUM — A panic in the tail session stops tailing for good, and the status still says "Tailing"
- `hosting/log_tail.rs:54-73` (contract), `ro-app/src/background/log_tail.rs:10-19`, `ro-app/src/main.rs:101-113`
- C#'s `ExecuteAsync` caught every exception, set Faulted and retried. In Rust, a panic in parsing or
  publishing unwinds the spawned task. The panic hook only logs it. The retry `loop` is inside that
  task, so tailing ends for the rest of the run, while `AgentStatus` keeps the `Tailing` it set in
  `find_log`. Items 6 and 7 are concrete panics that can reach this path in debug builds.
- Fix: run each session as a child task (`tokio::spawn(tail_session(..))`, with cloned `Arc`s) and
  map `JoinError::is_panic()` to `Faulted` plus a retry. Another option is
  `FutureExt::catch_unwind` around the session.

### 3. MEDIUM — Saving overlay settings deletes the rest of a hand-edited settings.json (inherited)
- `hosting/overlay_settings_store.rs:100-108` compared with `hosting/settings_file.rs:57`
- Startup loads settings.json through `json_text::relax`, which accepts `//` and `/* */` comments
  and trailing commas, as `AddJsonFile` did. `OverlaySettingsStore::read` parses the same file with
  strict `serde_json`. Scenario: the user adds `// my path` or leaves a trailing comma in
  settings.json. The agent starts normally. The first change to scale, offset or a key in Settings
  makes `read` fail, log "replacing them", and write a file that contains only `{"Overlay": …}`.
  `Agent.RustPath`, `Backend`, `Relay` and `Serilog` are silently deleted, and the next launch runs
  without them. The comment's reason ("Configuration already failed to load it") is wrong here. C#
  `JsonNode.Parse` was strict too, so the C# had the same bug.
- Fix: parse with `serde_json::from_slice(&json_text::relax(&bytes))`, which also handles the BOM.
  The comments are still lost on rewrite, but the data stays.

### 4. MEDIUM — Settings writes are not atomic, and a torn file stops the agent from starting (inherited)
- `hosting/overlay_settings_store.rs:88` (`std::fs::write`), `ro-app/src/main.rs:44-46`
- `std::fs::write` truncates the file, then writes. Every slider step in the Settings form calls
  `update` and so rewrites the file (`ro-app/src/state/settings_form.rs:53-77`), which keeps the
  exposure window open often. A crash, kill or power loss mid-write leaves an empty or half-written
  settings.json. On the next launch `load_layered` returns `SettingsError::Json`, and `main` treats
  that as fatal. The agent will not start until the user finds and deletes the file.
- Fix: write `settings.json.tmp` in the same directory, `sync_all`, then `std::fs::rename` over the
  target. On Windows this is `MoveFileExW(REPLACE_EXISTING)`, which is atomic on one volume.
  Optionally, quarantine an unparsable user settings.json at startup (rename it to `.bad`) and log
  it instead of failing.

### 5. LOW — A UTF-8 character split across two polls corrupts names (documented TODO)
- `logs/log_tailer.rs:76-85`
- Each poll decodes its chunk on its own with `from_utf8_lossy`. If the game's buffered write ends
  inside a multi-byte character (Cyrillic or CJK player names in a multi-KB combatlog dump), the
  line comes out with U+FFFD in place of that character. Downstream, the corrupted `attacker`/`target`
  no longer equals the clean copy in the next dump, so `CombatLogDeduplicator` emits the hit twice,
  and name→SteamID resolution misses. C# had the same behaviour, hence the TODO. The fix is small
  because Rust already holds raw bytes.
- Fix: before decoding, cut the chunk at the last complete UTF-8 boundary (scan back at most 3 bytes
  for a lead byte whose sequence is incomplete) and subtract the leftover from `position`, or carry
  it in a `Vec<u8>`. Keep the `partial: String` logic as it is.

### 6. LOW — A malformed combat row offset panics in debug and gives a garbage timestamp in release
- `timespan.rs:28-32`, called from `parsing/combat_log_parser.rs:36,68`
- `parse_suffixed` accepts any double, including `1e20s` and `Infinitys` (`parse_double` allows
  "Infinity"). In `add_seconds`, `(integral as i64)` saturates and `* TICKS_PER_SECOND` overflows.
  Debug builds panic, and through item 2 that stops tailing. Release builds wrap silently and emit a
  `CombatEvent` dated thousands of years away, which then also skews `prune`'s cutoff in the
  deduplicator (`combat_log_parser.rs` feeds `combat_log_deduplicator.rs:62-65`). C# `AddSeconds`
  threw `ArgumentOutOfRangeException`, which ended in fault and retry.
- Fix: use `checked_mul`/`checked_add` and `DateTime::checked_add_signed`, return `Option`, and drop
  the row when the result is `None`. Optionally reject non-finite offsets in `try_parse_row`.

### 7. LOW — A large TimeSpan in settings overflows during parsing
- `timespan.rs:83-87`
- `days * TICKS_PER_DAY` and the following sum use unchecked i64 arithmetic. A value such as
  `"PollInterval": "99999999"` or `"DedupeRetention": "20000000.00:00:00"` (a user trying to say
  "forever") panics at startup in debug. In release it wraps to an arbitrary, possibly negative,
  span. .NET rejected these values with `OverflowException`.
  - A negative `DedupeRetention` means `prune` keeps nothing.
  - A negative `PollInterval` makes `to_std()` fail, so the tailer silently falls back to 150 ms
    (`hosting/log_tail.rs:64`).
- Fix: use checked arithmetic and return `None` on overflow, which becomes a `SettingsError::Section`.

### 8. LOW — Blocking file I/O and unbounded synchronous batches on a tokio worker
- `logs/log_tailer.rs:61-85`, `hosting/log_tail.rs:66-72`
- `read_available` runs `open`, `metadata` and `read_to_end` synchronously inside an async fn. With
  `ReplayFromStart`, the whole log is read, decoded, split, parsed and published in one pass. Lossy
  `publish` never yields (only a full Lossless queue does), so that worker thread is held for the
  whole replay. Other tasks on it, such as relay heartbeats and the UI pump, stall.
- Fix: run `read_available` through `tokio::task::spawn_blocking`, or cap each read (for example
  1 MiB per call) and `tokio::task::yield_now()` every N lines in `tail_session`.

### 9. LOW — Any fault restarts the tail at EOF and skips what was written meanwhile (inherited)
- `hosting/log_tail.rs:63-64`, `logs/log_tailer.rs:69-71`
- Every session builds a fresh `LogTailer` with `started=false`, so after any `TailError` (an I/O
  hiccup opening the log during the game's relaunch truncation, or item 1) it jumps to the current
  end of the file. Everything written during the 5 s back-off is lost. With `ReplayFromStart=true`,
  the opposite happens: every retry replays and republishes the whole log with a fresh deduplicator,
  so duplicate combat events go to the relay.
- Fix: keep the `LogTailer` (position plus partial) across retries in the ro-app loop, and rebuild
  it only when `locate()` returns a different log path. Pass `from_start` only to the first session.

## Checked and matching C# (not findings)
- Truncation and restart detection (`length < position`), carrying partial lines, CRLF trimming,
  stripping the BOM at every chunk start (C# built a new StreamReader each poll), and the delay
  schedule in `next_lines` all match `LogTailer.cs`.
- On Windows, `File::open` and `fs::read` share READ|WRITE|DELETE, which is at least as permissive as
  the C# `FileShare.ReadWrite|Delete`. `ClientConfig`/`KeyBindings` reads are more permissive than
  C#'s `File.ReadLines`.
- No slicing panics on non-ASCII input: every `&str` slice index sits after an ASCII byte (`|`, `:`,
  space), or comes from `get()`.
- Combat parser index arithmetic: n ≥ 14 and the guard `weapon_idx <= 1` hold with and without
  `info`. `split_names` with `end - start == 1` behaves like C#.
- Block timestamps are an exact `yyyy-MM-dd'T'HH:mm:ss.fff'Z'` read as UTC. The regexes are anchored
  the same way. The correlator and deduplicator keep the same order, the same NaN semantics, and the
  same prune anchor (the last row added).
- EventQueue wakeups: `Notified::enable()` comes before the state check, so no wakeup is lost. Order
  within each subscriber is preserved.
- The Steam locator follows the same registry order and `libraryfolders.vdf` regex, with configured
  path precedence.
- The settings layer binds TimeSpan from a number as days (`"150"` → 150 days), like C#.
