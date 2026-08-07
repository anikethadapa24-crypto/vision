# ROADMAP.md — Vision Desktop

This is the build plan for Vision and the process we follow while executing it. `PRD.md` defines *what* and *why*; `ARCHITECTURE.md` defines the *system*; this file defines the *order of construction* and stays a living checklist — check items off as we complete them rather than treating it as a static plan.

## 0. How We Work Through This Roadmap

1. **Walking skeleton first.** Before deepening any one component, get a thin end-to-end path connected (client → daemon → storage → back to client). Milestone 1 exists to prove the wiring, not to be useful.
2. **Build order follows the data flow in `ARCHITECTURE.md` §6.** You can't retrieve before you ingest, you can't synthesize before you can retrieve, and you don't add a second content source until one source works end-to-end. Milestones are ordered accordingly — do not skip ahead to a "cooler" feature out of order.
3. **One milestone = one demoable checkpoint.** Nothing is marked `[x]` until it actually runs and its exit criteria are verified, not just implemented.
4. **A milestone doesn't start until the previous one's exit criteria are met**, unless we explicitly agree to defer something and note the reason inline.
5. **Each milestone below has the same shape:** Goal, Tasks, Depends On, Exit Criteria, Maps To (which ARCHITECTURE.md service/flow it implements). When we start a milestone, I'll pull its tasks into TaskCreate/TaskUpdate for in-session tracking; this file stays the durable record across sessions.
6. **Phases map to the PRD's roadmap** (§11) but are broken into buildable milestones — Phase 1 here delivers the PRD's MVP, Phase 2 delivers its Beta, Phase 3 its Scale.

---

## Phase 0 — Foundation

### M0. Repo & Tooling Bootstrap
- [x] Initialize repo structure: `daemon/` (Rust), `shell/` (Tauri UI), `extension/` (browser), `docs/` (PRD, ARCHITECTURE, ROADMAP)
- [x] Set up Cargo workspace for daemon + shared crates
- [x] Set up Tauri project for Tray App / Query UI
- [x] CI: build + lint + test on Windows/macOS/Linux runners — `.github/workflows/ci.yml` ran for real on GitHub Actions (run [30762809939](https://github.com/anikethadapa24-crypto/vision/actions/runs/30762809939)), all 6 matrix jobs (daemon×3, shell×3) green
- [x] Decide and pin: embedded graph DB (Kùzu per `ARCHITECTURE.md` §9.1), embedded vector index (LanceDB), local LLM runtime (`llama.cpp`, ONNX as pluggable fallback)
- **Depends on:** —
- **Exit Criteria:** `cargo build` and `tauri build` succeed on all 3 target OSes in CI; empty daemon binary runs and exits cleanly. **Met** — CI run 30762809939, all 6 matrix jobs passed.
- **Maps to:** §7 (Process & Deployment Topology)

### M1. Daemon Skeleton + Local API Gateway
- [ ] Daemon process with autostart registration stub (Windows Service / launchd / systemd — registration only, no logic yet)
- [ ] Single-instance lock file (per ARCHITECTURE.md §7 "Single writer" principle)
- [ ] Local API Gateway: gRPC server over UDS (macOS/Linux) / named pipe (Windows)
- [x] Define `.proto` contracts for `IngestEvent`, `Query`, `Permissions`, `Audit` (stubs returning fixed responses) — `vision-proto` (tonic/prost, vendored `protoc`), fixed-response impl in `vision-core::VisionApiService`, round-tripped over real gRPC (loopback TCP, not yet the production UDS/named-pipe transport) in 8 passing tests
- [ ] Tray App connects to daemon, shows connection status in tray icon
- **Depends on:** M0
- **Exit Criteria:** Tray App shows "connected" against a running daemon; killing/restarting the daemon is handled gracefully by the client (reconnect, not crash).
- **Maps to:** §3 (API Gateway), §4.1–4.2 (transport + contract)

### M2. Config, Permissions & Audit Stores
- [ ] `config.sqlite` schema: folder/app permission grants, feature flags, install token
- [ ] `audit.sqlite` schema: append-only indexed-item log with soft-delete
- [ ] Permissions UI in Tray App settings: pick folders to index, grant/revoke
- [ ] `Permissions.{Get,Set,Revoke}` and `Audit.{List,Delete}` RPCs implemented for real (no more stubs)
- **Depends on:** M1
- **Exit Criteria:** User can grant a folder in the UI, see it persisted after daemon restart, and revoke it; audit log page lists entries (empty at this stage) without erroring.
- **Maps to:** §5.2 (config.sqlite, audit.sqlite), §5.3 (deletion coordination groundwork)

---

## Phase 1 — MVP (target: PRD §11 Phase 1, Months 1–4)

### M3. Filesystem Watcher + Blob Store
- [ ] Platform watchers wired (`ReadDirectoryChangesW` / `FSEvents` / `inotify`) scoped to permitted folders from M2
- [ ] `IngestEvent` fires on create/modify/delete
- [ ] Content-addressed Blob Store (`blobs/`, SHA-256 keyed) — raw bytes in, for now no extraction
- [ ] Resource throttling stub: pause watcher callbacks under high CPU (basic, refine in M11)
- **Depends on:** M2
- **Exit Criteria:** Saving a file in a permitted folder produces a blob within 5s; audit log records the event.
- **Maps to:** §6.1 Flow A (first half)

### M4. Embedded Graph DB Integration
- [ ] Stand up Kùzu inside the daemon; define initial schema: `Document` node type, `indexed-in` edge to a `Folder`/`Source` node
- [ ] On ingest, upsert a `Document` node referencing its blob hash
- [ ] `GetGraph(scope)` RPC returns real data (even if just flat document lists at this stage)
- **Depends on:** M3
- **Exit Criteria:** Indexing 100 files produces 100 correctly-linked graph nodes, queryable via `GetGraph`.
- **Maps to:** §5.2 (Graph DB row), §6.1 Flow A (graph write step)

### M5. Text Extraction Pipeline
- [ ] Extractors: plain text, PDF, Markdown, code files first (Word/Google Docs, Jupyter, OCR deferred to a later pass in this milestone if time allows, else M9)
- [ ] Extracted text written to Blob Store alongside raw bytes
- [ ] Chunking strategy defined (size + overlap) for downstream embedding
- **Depends on:** M3
- **Exit Criteria:** For each supported file type, extracted-text blob exists and is readable; unsupported types degrade gracefully (indexed as metadata-only, not crash).
- **Maps to:** §6.1 Flow A (extraction step)

### M6. Local LLM Runtime + Embedding + Vector Index
- [ ] Model Cache directory + first-run model download flow (embedding model first; generation model can follow)
- [ ] Embedding Service generates vectors for chunks from M5
- [ ] LanceDB vector index stores chunk embedding + blob pointer + graph node ID
- **Depends on:** M4, M5
- **Exit Criteria:** After indexing, a raw cosine-similarity query against the vector index returns sane nearest neighbors for a known test query.
- **Maps to:** §3 (Embedding Service, LLM Runtime), §5.2 (Vector Index row)

### M7. Query Orchestrator v1 — Retrieval Only
- [ ] Hotkey opens Query UI; text input wired to `Query()` RPC
- [ ] Orchestrator: embed query → ANN search → return ranked source list (paths + snippets, **no LLM synthesis yet**)
- [ ] Query UI renders results list with file path + snippet + timestamp
- **Depends on:** M6
- **Exit Criteria:** Typing a query about a previously-indexed file surfaces it in the results within 2s.
- **Maps to:** §6.2 Flow B (first half), §5.4 (query UI, minus synthesis)

### M8. Answer Synthesis + Source Attribution
- [ ] Load generation-capable local LLM into the runtime alongside the embedding model
- [ ] Orchestrator assembles retrieved chunks into a context window, streams a synthesized answer back over the gRPC server-stream
- [ ] Query UI renders streamed tokens + cites source doc/timestamp/path per PRD §5.5
- **Depends on:** M7
- **Exit Criteria:** Asking "explain the document about X" returns a synthesized answer with at least one correct citation, matching PRD examples in §5.4.
- **Maps to:** §6.2 Flow B (full), §5.5 (Source Attribution)

### M9. Entity/Relationship Extraction → Graph-Aware Retrieval
- [ ] Upgrade ingestion to extract entities/relationships via LLM (not just Document nodes) — concepts, people, projects; edges `cites`, `relates-to`, `authored-by`, `used-in`
- [ ] Typed nodes with metadata (status, timestamps, tags) per PRD §5.3
- [ ] Orchestrator upgraded to hybrid retrieval: vector search for entry points, then graph traversal for connected context (true GraphRAG, not vector-only)
- **Depends on:** M8
- **Exit Criteria:** A query about a concept surfaces not just the directly-matching document but linked related documents via graph traversal; A/B comparable against M8's vector-only baseline (PRD §8.2).
- **Maps to:** §5.3 (Knowledge Graph Memory Architecture), §6.2 Flow B (graph traversal step)

### M10. Deletion & Audit Completion
- [ ] Coordinated delete across graph + vector + blob + audit row in one transaction boundary (ARCHITECTURE.md §5.3)
- [ ] Audit Log UI: view indexed items, delete individual items or time ranges
- **Depends on:** M9
- **Exit Criteria:** Deleting an item from the Audit UI removes it from all four stores; a subsequent query no longer surfaces it.
- **Maps to:** §5.3 (Encryption & deletion), PRD §5.7

### M11. Performance & Resource Hardening
- [ ] Real resource throttling (CPU/memory pressure-aware worker pool for ingestion)
- [ ] Benchmark against PRD §7.3 targets: <5s ingest latency, <2s query latency (p90), <10% CPU / <500MB RAM idle
- [ ] Load test at 100K+ nodes / 500K+ edges (PRD §7.3 scalability target)
- **Depends on:** M10
- **Exit Criteria:** Automated benchmark suite passes all PRD §7.3 numeric targets on the "Recommended" spec tier.
- **Maps to:** §7.3 Performance Requirements

### M12. Packaging & Installers — MVP Release Candidate
- [ ] `.exe` (Windows), `.dmg` (macOS), `.deb`/`.rpm` (Linux) with system requirements check
- [ ] Code signing for all installers (PRD §7.4)
- [ ] At-rest encryption wired for the full data directory (OS-backed key per ARCHITECTURE.md §5.3)
- [ ] First-run onboarding: permission grants, model download, hotkey tutorial
- **Depends on:** M11
- **Exit Criteria:** Clean install → onboarding → first successful query, on all 3 OSes, by someone who hasn't seen the codebase.
- **Maps to:** PRD §5.1, §11 Phase 1 completion

**Phase 1 exit = PRD's MVP definition met: text-based query interface, local indexing, embedded graph, source attribution, installers for all 3 OSes.**

---

## Phase 2 — Beta (target: PRD §11 Phase 2, Months 5–8)

### M13. Browser Extension
- [ ] WebExtension (MV3) for Chrome/Firefox/Edge via Native Messaging host
- [ ] Safari bridge via localhost WebSocket + per-install auth token (ARCHITECTURE.md §4.3 exception)
- [ ] Tab/history capture feeding `IngestEvent`, same pipeline as M3 onward
- **Depends on:** M9 (needs full ingestion+graph pipeline, not just M3)
- **Exit Criteria:** Visiting a page and later querying about it surfaces it with correct source attribution (URL, not file path).
- **Maps to:** §3 (Browser Extension), §4.1 transport row

### M14. Voice: Wake Word + STT
- [ ] On-device wake-word model ("Hey Vision") in the always-on Voice Capture client
- [ ] STT engine (Whisper.cpp-class) wired into daemon, audio streamed only post-trigger
- [ ] Query UI opens on wake word same as hotkey path (M7 reused)
- **Depends on:** M8
- **Exit Criteria:** Saying "Hey Vision" from an idle state opens the query UI and correctly transcribes a test question within acceptable latency.
- **Maps to:** §3 (Voice Capture, STT), PRD §5.4

### M15. Multi-Turn Conversations
- [ ] Session-scoped conversation history maintained in Query Orchestrator
- [ ] Follow-up queries resolve references to prior turns ("what about the second one?")
- **Depends on:** M9
- **Exit Criteria:** A 3-turn conversation with pronoun/reference follow-ups produces coherent, correctly-scoped answers.
- **Maps to:** PRD §5.4 Multi-Turn Conversations

### M16. Project Workspaces & Concept Map Visualizer
- [ ] Project subgraph view (all files/notes/decisions/people for a project)
- [ ] Visual graph explorer using `GetGraph` (built in M4, richer by now)
- [ ] Timeline view showing versioned evolution of a topic
- **Depends on:** M9
- **Exit Criteria:** Selecting a project in the UI renders its subgraph and timeline without a full-graph fetch (scoped query performance acceptable).
- **Maps to:** PRD §5.6

### M17. Premium Tier: Cloud Sync Agent
- [ ] Opt-in encrypted snapshot sync (`graph/`, `vectors/`, never raw `blobs/`) per ARCHITECTURE.md §5.3
- [ ] Billing/subscription integration for the $20–30/mo tier (PRD §3, §9)
- [ ] Advanced graph analytics dashboard (premium-gated)
- **Depends on:** M12
- **Exit Criteria:** Enabling sync on one device and restoring on a second produces a working, queryable graph; disabling sync leaves local-only operation unaffected.
- **Maps to:** §3 (Cloud Sync Agent), §5.3, PRD §5.7 / §9 Monetization

**Phase 2 exit = PRD's Beta definition met: browser capture, voice, multi-turn, workspaces, premium tier live.**

---

## Phase 3 — Scale (target: PRD §11 Phase 3, Months 9–12)

### M18. Third-Party App Integrations
- [ ] Slack, Discord, Zoom (transcripts), VS Code, JetBrains connectors — each as an ingestion source feeding the same pipeline established by M3–M9
- **Depends on:** M13 (proves the "external source" pattern via the browser extension first)
- **Exit Criteria:** Each integration ingests real activity and is queryable with correct source attribution.
- **Maps to:** PRD §10 Dependencies (Third-Party Integrations)

### M19. Team Sharing & Collaborative Graphs
- [ ] Shared-subgraph model, access control, conflict resolution for concurrent edits (flagged as an open question in ARCHITECTURE.md §9)
- **Depends on:** M17
- **Exit Criteria:** Two users on a shared project graph both see updates without data loss or divergence.
- **Maps to:** PRD §4 Secondary Personas (Teams), §11 Phase 3

### M20. Public API
- [ ] Documented, authenticated API surface for third-party developers to build on the graph
- [ ] Rate limiting, API key management
- **Depends on:** M17
- **Exit Criteria:** A third-party sample app can authenticate and read a user's (permissioned) graph data end-to-end.
- **Maps to:** PRD §11 Phase 3

**Phase 3 exit = PRD's Scale definition met: full integration surface, team features, public API, targeting 50K+ downloads / 10K+ DAU (PRD §9).**

---

## Tracking Convention

- Check off `[ ]` → `[x]` only after the milestone's Exit Criteria is demonstrably true, not on "code complete."
- If a milestone is reordered or descoped, note it inline (`~~strikethrough~~` + reason) rather than deleting it, so the history of plan changes is visible.
- Re-open this file at the start of each work session to pick the next unchecked milestone whose dependencies are satisfied.
