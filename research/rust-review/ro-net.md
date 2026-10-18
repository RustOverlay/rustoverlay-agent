# ro-net review (Rust port vs C# `Core/{Enrollment,Teams,Relay}`)

Scope: `crates/ro-net/src/**`, `build.rs`, `Cargo.toml`, `proto/rustoverlay/v1/team.proto`. Wire
contracts checked against `backend/api` (FastAPI/pydantic) and `backend/relay` (starlette WS).
"Parity" means the C# agent has the same bug. The port did not introduce it, but the fix is cheap now.

## Findings (ranked)

### 1. HIGH: a live relay session ignores Disconnect, team switch and sign-out (parity)
- `crates/ro-net/src/relay/relay_uplink.rs:77-106`. The team and token are read once per session (`:78`). Nothing ends a session that is already connected.
- Scenario: the user is connected to team A and clicks Disconnect. `ro-app/src/state/team.rs:208-223` deletes `active-team.json` and clears the roster. The socket stays open, though. Heartbeats every 30 s keep it alive past Cloudflare's idle cut, so loadout, vitals and combat keep streaming to team A indefinitely. Relay snapshots (`read`, `:115`) then refill the roster the UI just cleared. The same happens when the user switches to team B (they stay in A), when the team is orphaned on refresh (`team.rs:186`), and after `AuthState::sign_out`. That last case contradicts the `auth_state.rs:36-39` claim that the uplink "falls silent within one reconnect delay".
- This breaks the module's own contract: "Nothing leaves the machine unless a team is active" (`relay_uplink.rs:22`).
- Fix: give `ActiveTeamStore` and `AuthState` a change signal, e.g. a `tokio::sync::watch<u64>` generation bumped in `save()` and `sign_out()`. Add a branch to the `select!` at `:99` that returns once the generation moves. As a cheaper alternative, re-check `teams.load() == Some(team)` and `tokens.load().is_some()` on each `send_loadout` tick, and end the session on a mismatch.

### 2. MEDIUM: a stale 401 after re-sign-in deletes the new token (parity)
- `crates/ro-net/src/teams/teams_client.rs:153-155` together with `crates/ro-net/src/enrollment/auth_state.rs:40-52`.
- Scenario: the token is revoked. The first 401 sets `signed_out`, and the user signs in again (`session.rs:94` calls `sign_in()`, so the flag flips back). A request still in flight with the old bearer (`team.refresh()` or any team call) then returns 401. `sign_out()` sees `signed_out == false` and runs `tokens.clear()`, which deletes the freshly enrolled token. The user is bounced back to the sign-in screen right after signing in.
- Fix: `sign_out(rejected: &str)`. Clear only when `tokens.load()` still holds that same token string. `send_raw` already has `token.token` (`:125`).

### 3. MEDIUM: one bad connection aborts the whole browser sign-in, and an idle one blocks it (parity)
- `crates/ro-net/src/enrollment/loopback_callback.rs:59-75`.
- Every per-connection I/O error (`?` on `accept`, `read_request_line`, `write_all`, `flush`) ends `wait_for_code`, and with it `enroll()`. Chromium-based browsers open speculative preconnect sockets to the redirect target. If such a socket is closed or reset after the 404 is written, `write_all`/`flush` can return `ConnectionReset`/`BrokenPipe`, and sign-in fails with an I/O error. A preconnect that is idle and sends nothing also blocks the serial loop in `read_request_line` (no read timeout). The real `GET /done?code=` then waits in the backlog until the browser drops the idle socket. Meanwhile the 60 s single-use code (`enrollment_service.rs:150`) is ageing.
- Fix: handle each accepted stream in its own task (or `FuturesUnordered`) with a short read timeout (around 5 s). Log and ignore per-connection errors. Only `accept()` errors should be fatal.

### 4. MEDIUM: TLS roots and proxy differ from the C# agent, so some networks can no longer connect
- `crates/ro-net/Cargo.toml:19` (`reqwest`: `rustls-tls`, default features off) and `:23` (`tokio-tungstenite`: `rustls-tls-webpki-roots`). The lockfile has only `webpki-roots`. It has no `rustls-native-certs`/`rustls-platform-verifier` and no system-proxy support (no `windows-registry` under reqwest).
- The C# agent used SChannel with the Windows certificate store and `HttpClient.DefaultProxy`/`ClientWebSocketOptions.Proxy`, which read the Windows proxy settings.
- Scenario: antivirus HTTPS/WebSocket scanning (Kaspersky, ESET, Avast and similar, common on gaming PCs) or a corporate TLS-inspection root is trusted by Windows but not by Mozilla's bundle. Sign-in, team calls and the relay then all fail with `UnknownIssuer`, where the C# agent worked. A user behind a Windows-configured proxy also cannot reach the API. The relay ignores proxies completely.
- Fix: use `rustls-platform-verifier` (or the `rustls-tls-native-roots` features on both crates), and enable reqwest's system-proxy feature. The WebSocket proxy needs more work; at minimum, document it.

### 5. LOW: the relay socket keeps Nagle enabled, unlike .NET
- `crates/ro-net/src/relay/relay_uplink.rs:84`: `connect_async_with_config(.., false)`. That third argument is `disable_nagle`, so `false` leaves Nagle on. .NET's SocketsHttpHandler (which ClientWebSocket uses) sets `NoDelay = true`.
- Effect: small frames (heartbeat, combat, vitals) can wait behind delayed ACKs (up to about 200 ms on Windows) when sent back to back. That is extra latency for teammates' HUD updates.
- Fix: pass `true`.

### 6. LOW: fixed 5 s retry after a server `Error` frame hammers the API, and two installs ping-pong (parity)
- `crates/ro-net/src/relay/relay_uplink.rs:117-119` and `:73`. On the relay side, `connection.py:49-52`/`:97-100` sends `bad_token`, `room_full`, `version_unsupported` or `replaced`.
- `bad_token` (for example after a kick), `room_full` or `version_unsupported` reconnects every 5 s forever. Each attempt costs one `api.verify_join` call on the backend, which is 12 per minute per agent, for good. Two installs on the same Steam account keep replacing each other every 5 s.
- Fix: exponential backoff with jitter after an `Error` frame, capped at a few minutes. Stop retrying on `version_unsupported` and `replaced` until the team or token changes (this pairs with the fix for finding 1).

### 7. LOW: no connect timeout and no read-liveness timeout on the relay (parity)
- `crates/ro-net/src/relay/relay_uplink.rs:83-84` (no timeout on TCP, TLS or upgrade) and `relay_connection.rs:74` (`stream.next()` with no idle bound).
- The relay sends its own heartbeat and a periodic snapshot (`connection.py:211-224`). If the link goes half-open (Wi-Fi drop, NAT rebind), the agent only notices once TCP retransmission gives up, which can take minutes. During that time `connected()` stays true and the roster freezes.
- Fix: wrap `connect_async_with_config` in `tokio::time::timeout` (about 15 s). End the session if no server frame arrives within about 3× the relay heartbeat interval.

### 8. LOW: the loopback code is not bound to this sign-in, so login CSRF is possible (parity, needs backend)
- `crates/ro-net/src/enrollment/loopback_callback.rs:80-88` and `enrollment_service.rs:100-121`.
- Any local origin that finds the port during the 5-minute window can deliver its own freshly minted `code` (for example a web page navigating to `http://127.0.0.1:<port>/done?code=…`). The agent exchanges it and enrolls as the attacker's account. The victim's team activity then flows to an account the attacker controls. `/auth/steam/start` takes only `cb` (`routers/auth.py:22-28`), so the code is never bound to the agent that started the flow.
- Fix: PKCE-style binding. The agent sends `sha256(verifier)` on `/auth/steam/start` and the `verifier` on `/auth/exchange`, and the API rejects a mismatch.

### 9. LOW: Rust release builds will report agent version 1.0.0.0
- `crates/ro-net/build.rs:11-13`. Nothing sets `RUSTOVERLAY_VERSION`; `.github/workflows/release.yml:21-31` only passes `-p:Version` to `dotnet publish`. The API records `install.agent_version` from `/auth/exchange`, and the relay receives `JoinRoom.agent_version`. Both will read 1.0.0.0 for every Rust release, which defeats any future `version_unsupported` gate.
- Fix: when the Rust release job is added, export `RUSTOVERLAY_VERSION=${GITHUB_REF_NAME#v}`.

### 10. LOW: the join code is placed in the URL path unescaped (parity)
- `crates/ro-net/src/teams/teams_client.rs:85`. `Url::join` interprets `?`, `#`, `/` and `..` in user input. For example, `..` resolves to `GET /v1/teams/`, which FastAPI redirects to the team list, and then gives a confusing "unreadable response" error. `ab?x` drops the code entirely.
- Fix: percent-encode with a path-segment set (and trim) before formatting, or use `url.path_segments_mut().push(join_code)`.

### 11. LOW: a crash during the token write forces a re-sign-in (parity)
- `crates/ro-net/src/enrollment/dpapi_token_store.rs:54`: `fs::write` truncates and then writes. A crash or power loss in between leaves a partial blob. `load()` then returns `None` (`:45-49`) and the user has to sign in again. `active_team_store.rs:40` behaves the same way.
- Fix: write to `install.token.tmp`, then `fs::rename` (on Windows this replaces the file atomically through MoveFileEx).

### 12. LOW: the relay's per-session lossless subscription can fault the log tail (parity)
- `crates/ro-net/src/relay/relay_uplink.rs:164` with `ro-core/src/hosting/game_event_bus.rs:63-69` and `ro-core/src/hosting/log_tail.rs:69`.
- Scenario: the socket stalls, so `send_combat` blocks on `connection.send`. After 512 queued combat rows, the publisher blocks on the relay queue (which also stalls UI events). When the session finally errors, dropping the subscription completes the queue. The waiting `publish` returns `ChannelClosed`, the tail session ends as Faulted, and `ro-app` restarts tailing from EOF 5 s later, losing the rows in between. This path needs a stalled socket and a burst of 512 or more combat rows, so it is rare. It is also not a race window.
- Fix (in ro-core): when a write hits a queue that was disposed (not bus-completed), skip that subscriber instead of failing `publish`. Alternatively, give the relay a session-independent subscription that it drains, or drops, while disconnected.

## Verified correct (no action)
- `proto/rustoverlay/v1/team.proto` is byte-identical to `rustoverlay-contracts` and to HEAD. Prost oneof/optional mapping matches what the relay emits (`protocol.py`).
- DPAPI compatibility: same entropy `RustOverlay.InstallToken.v1`, CurrentUser scope (no LOCAL_MACHINE flag), `CRYPTPROTECT_UI_FORBIDDEN` as .NET sets, null description. The JSON shape (`token`/`steam_id`/`display_name`) round-trips with `System.Text.Json` output, including `\uXXXX` escapes. Existing tokens will decrypt.
- JSON wire names match the pydantic schemas (`schemas/team.py`, `schemas/auth.py`, `core/errors.py`). Timestamps are `timestamptz`, so pydantic emits an offset and `DateTime<FixedOffset>` parses it. No backend field is nullable, so serde's null-vs-missing difference does not arise.
- `unix_millis` / `timestamp_millis` floor as `ToUnixTimeMilliseconds` does.
- The rustls stack has only `ring`, so there is no CryptoProvider-ambiguity panic. Server pings get ponged from the read path (tungstenite flushes queued pongs on read; the split halves share a BiLock).
- The 64 KiB inbound cap matches C#. Undecodable frames end the session instead of crashing the service (an improvement on C#, where `InvalidProtocolBufferException` was uncaught).
- HTTP timeout is set on the shared client (`ro-app/src/host.rs:51`). The enrollment exchange is additionally bounded by the sign-in deadline.
