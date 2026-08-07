# TASKS.md — Vision Execution Queue

`ROADMAP.md` maps milestones (weeks each). This file is the granular, near-term work queue those milestones get broken into — tasks sized for a single sitting, ordered top-to-bottom, kept current. It only ever holds the current milestone plus roughly the next one and a half — further out than that, granularity is guesswork that'll just need rewriting when we get there. "Greatest project of all time" isn't a slogan here — it's the Excellence Bar in §1, applied to every single task before it gets checked off.

## 1. Excellence Bar — Definition of Done for Every Task

No task is checked off until all of these are true, not just "code written":

- [ ] Builds clean, no warnings suppressed or ignored
- [ ] Unit tests written and passing for any new logic (not just happy path — the failure mode too)
- [ ] Matches the contracts already committed to: service boundaries in `ARCHITECTURE.md` (no client talking to storage directly, no second writer to the graph), tokens/components in `UI.SPEC.md` (no ad hoc colors or one-off styling)
- [ ] Manually exercised end-to-end, not just unit-tested — backend: hit the RPC from a real client; UI: click through the actual state in a running app
- [ ] No TODO left behind without a corresponding task added to §3 or §4 of this file
- [ ] `PRD.md` / `ARCHITECTURE.md` / `ROADMAP.md` / `UI.SPEC.md` updated in the same change if reality diverged from what they said — the docs stay true, not aspirational
- [ ] Commit message says *why*, matching the repo's commit convention already in use

If a task can't clear this bar in one sitting, it's too big — split it before starting, don't lower the bar.

## 2. Right Now — Immediate Queue (Phase 0 → ROADMAP.md M1–M2)

Pull from the top. Nothing below M2 is expanded yet — see §3.

### M1. Daemon Skeleton + Local API Gateway
- [x] Add gRPC dependency (`tonic`), define initial `.proto`: `IngestEvent`, `Query`, `Permissions`, `Audit` (stub messages, fixed responses) — `daemon/vision-proto/proto/vision.proto` + `tonic-prost-build` codegen (vendored `protoc` via `protoc-bin-vendored`, so no system package needed on any CI runner); `VisionApiService` in `daemon/vision-core/src/service.rs` implements all 7 RPCs with fixed responses; verified with 5 unit tests plus 3 integration tests in `daemon/vision-core/tests/grpc_service.rs` that round-trip a real generated client against a real server over loopback TCP (real protobuf wire serialization, not in-process trait calls) — `cargo build/test/clippy -D warnings` all clean across the workspace. Loopback TCP is test-only scaffolding, not the production transport; that's the next two tasks (UDS / named pipe).
- [ ] Implement UDS transport (macOS/Linux)
- [x] Implement named-pipe transport (Windows) — `daemon/vision-daemon/src/transport/windows.rs`: `Connected` wrapper + `incoming()` stream for the server side, a `tower::Service<Uri>` `NamedPipeConnector` for the client side (tonic has no built-in named-pipe support, so this mirrors tonic's own UDS connector pattern). `vision_daemon::serve()` in the new `vision-daemon/src/lib.rs` wires it to `VisionApiService`; `main.rs` is now a thin wrapper so the transport is testable. `#[cfg(unix)]` has a matching `serve()` stub (`unimplemented!`) purely so the workspace keeps compiling on macOS/Linux CI runners until the next task lands — no test exercises that path, so CI stays green. Verified: an integration test (`vision-daemon/tests/named_pipe.rs`) round-trips a real `VisionApiClient` over a real named pipe; separately ran the actual `vision-daemon.exe` and hit it from a standalone client binary (`examples/client.rs`) in a second process, got back the real fixed response. Did **not** get to manually verify graceful shutdown on Ctrl+C — this dev shell can't deliver a real console CTRL_C_EVENT to a child process, so `tokio::signal::ctrl_c()` in `main.rs` is implemented but its actual signal delivery is unverified; flagging rather than claiming it.
- [x] Single-instance lock file check on daemon startup (per `ARCHITECTURE.md` §7 "single writer" rule) — `daemon/vision-daemon/src/single_instance.rs` uses `std::fs::File::try_lock` (stable, OS-level exclusive lock: `flock`/`LockFileEx`) on `<base_data_dir>/daemon.lock`, so a stale lock can never wedge a future launch even after a crash — the OS releases it when the file handle closes, no matter how the process ends. `base_data_dir()` (new `daemon/vision-core/src/paths.rs`, all 3 OSes per `ARCHITECTURE.md` §5.1) added `daemon.lock` to the §5.1 tree diagram since it wasn't listed. Second instance refuses to start with a clear message and exit code 1 — does **not** yet proxy to the running instance as §7 describes; that's a real gap, tracked below rather than silently implemented as less than what the doc promises. Verified: 2 unit tests (contention + release-on-drop) plus manually running two real `vision-daemon.exe` processes side by side — second one printed "another vision-daemon instance is already running" and exited 1, confirmed `daemon.lock` created under `%LOCALAPPDATA%\Vision\`, confirmed the lock frees up and a fresh start succeeds once the first process is killed.
- [ ] Autostart registration stub per OS (launchd plist / systemd user unit / Windows Service) — registration only, no logic yet
- [ ] Tray app connects to daemon over the local transport, shows connected/disconnected state in the tray icon per `UI.SPEC.md` §5a
- [ ] Manual test: kill the daemon while the tray app is running, confirm reconnect without a client crash

### M2. Config, Permissions & Audit Stores
- [ ] `config.sqlite` schema + migrations
- [ ] `audit.sqlite` schema, append-only with soft-delete
- [ ] Implement `Permissions.{Get,Set,Revoke}` for real (no more stubs)
- [ ] Implement `Audit.{List,Delete}` for real (empty result sets are fine at this stage — the plumbing is what's under test)
- [ ] Settings window shell per `UI.SPEC.md` §5c (tab list), even if only the Permissions tab is functional yet
- [ ] Build the Permissions tab UI: folder-picker list, opt-in-by-default per `UI.SPEC.md` §5b/§5c
- [ ] Manual test: grant a folder, restart the daemon, confirm the grant persisted; revoke it, confirm it's gone

## 3. Next Up — Queued, Not Yet Expanded

Expand each into granular tasks here once the milestone before it is fully checked off — not before, or this list drifts out of sync with what we actually know at the time.

- [ ] Expand `ROADMAP.md` M3 (Filesystem Watcher + Blob Store) once M2 is done
- [ ] Expand `ROADMAP.md` M4 (Graph DB Integration) once M3 is done

## 4. Parking Lot

Ideas, cleanups, or tasks that surface mid-work but aren't part of the current milestone. Capture them here instead of derailing what's in progress — triage into §2/§3 at the start of the next session.

- `ARCHITECTURE.md` §7 says a second daemon instance should proxy to the running one, not just refuse to start. Implemented only the refuse-to-start half (`single_instance.rs`) — proxying means the second process would have to detect the first is alive, forward every RPC to it, and exit once done, which is real scope on top of the lock check. Revisit once there's an actual client (Tray App) that would benefit from not just erroring out.
- Two local commits (M1: proto/stub service, M1: named-pipe transport + single-instance lock) are sitting ahead of `origin/main`, unpushed as of 2026-08-07. Push once reviewed, and confirm the `#[cfg(unix)]` `serve()` stub doesn't break the macOS/Linux CI legs.

## 5. How to Keep This File Honest

- Check a task off only when §1's Excellence Bar is met — "compiles" is not "done."
- When every task in a milestone is checked, check that milestone's box in `ROADMAP.md` too, then move the milestone's section here into a `## Done` archive at the bottom of this file (so the active queue stays short and scannable) rather than deleting the history.
- Re-open this file at the start of every work session and start at the top of §2 — if §2 is empty, promote the next milestone from §3 into it before writing any code.
