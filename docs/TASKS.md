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

## 2. Right Now — Immediate Queue (Phase 0 → ROADMAP.md M0–M2)

Pull from the top. Nothing below M2 is expanded yet — see §3.

### M0. Repo & Tooling Bootstrap
- [x] Create repo skeleton: `daemon/`, `shell/`, `extension/`, `docs/`
- [x] Move `PRD.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `UI.SPEC.md`, `TASKS.md` into `docs/`
- [x] `git init`, initial commit, `.gitignore` for Rust/Node/Tauri build artifacts
- [x] Cargo workspace: `daemon/Cargo.toml` + crate skeleton (`vision-core`, `vision-proto`, plus `vision-daemon` bin) — builds, lints (`clippy -D warnings`), and tests clean locally
- [x] Tauri scaffold in `shell/` (React+TS) — `npm run tauri build` verified locally, produces working `.msi`/`.exe` installers
- [ ] CI workflow: build + lint + test matrix across `windows-latest` / `macos-latest` / `ubuntu-latest` — `.github/workflows/ci.yml` authored and its steps verified locally on Windows, but not yet run for real: **repo has no git remote yet**, so GitHub Actions has never executed. Leave unchecked until it's actually gone green on all 3 runners.
- [x] Resolve `ARCHITECTURE.md` §9's open question with a short written decision (Kùzu vs. Memgraph, LanceDB, llama.cpp vs. ONNX) — one paragraph each, committed to `docs/` (§9.1)
- [ ] Confirm an empty daemon binary builds and runs clean on all 3 CI runners — verified locally on Windows only (`vision-daemon.exe` builds and exits 0); macOS/Linux runners unverified until CI actually runs

### M1. Daemon Skeleton + Local API Gateway
- [ ] Add gRPC dependency (`tonic`), define initial `.proto`: `IngestEvent`, `Query`, `Permissions`, `Audit` (stub messages, fixed responses)
- [ ] Implement UDS transport (macOS/Linux)
- [ ] Implement named-pipe transport (Windows)
- [ ] Single-instance lock file check on daemon startup (per `ARCHITECTURE.md` §7 "single writer" rule)
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

- Repo has no git remote yet, so `.github/workflows/ci.yml` has never actually run. Push to GitHub and confirm the matrix goes green on all 3 OSes before checking off M0's CI task/exit criteria for real.

## 5. How to Keep This File Honest

- Check a task off only when §1's Excellence Bar is met — "compiles" is not "done."
- When every task in a milestone is checked, check that milestone's box in `ROADMAP.md` too, then move the milestone's section here into a `## Done` archive at the bottom of this file (so the active queue stays short and scannable) rather than deleting the history.
- Re-open this file at the start of every work session and start at the top of §2 — if §2 is empty, promote the next milestone from §3 into it before writing any code.
