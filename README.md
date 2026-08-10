# Vision

**The memory operating system for the AI era.**

Vision is a desktop utility (Windows/macOS/Linux) that runs quietly in the background, continuously turning your files, browser activity, and app usage into a queryable, interconnected knowledge graph. Hit a hotkey or say "Hey Vision," ask a question in plain English, and get an answer synthesized from everything you've ever worked on — with sources cited.

> **Status: functional prototype engine, no UI yet.** The daemon really ingests files, indexes them, and answers queries with real (not fixed-stub) cited results — driven today through a REPL client, not the Tray App/Query UI, which are still unbuilt. See [Current Status](#current-status) below for exactly what's real versus an interim stand-in.

## Why

Search on a modern computer still means matching filenames and keywords. It doesn't know that the PDF you read last week relates to the note you wrote this morning, and every AI assistant you talk to forgets you the moment the chat ends. Vision fixes that by indexing everything locally into a graph — not a flat search index — so retrieval follows the actual relationships between what you've read, written, and decided, not just word overlap.

## Documentation

Read in this order — each builds on the one before it:

| Doc | What it answers |
|---|---|
| [`PRD.md`](./docs/PRD.md) | What are we building, for whom, and why does it matter |
| [`ARCHITECTURE.md`](./docs/ARCHITECTURE.md) | What services exist, how they talk to each other, where data lives on disk |
| [`ROADMAP.md`](./docs/ROADMAP.md) | What order we build it in, milestone by milestone, and how we'll know each one is done |
| [`UI.SPEC.md`](./docs/UI.SPEC.md) | Every screen, design token, and component the front end is built from |
| [`TASKS.md`](./docs/TASKS.md) | The actual granular work queue we're pulling from right now |

If you're picking up work on this project, `TASKS.md` is the file to open first — it's the only one meant to be read every session.

## How It Works, in Short

Vision is one background daemon (Rust) plus a set of thin client surfaces (tray app, floating query overlay, browser extension) that only ever talk to that daemon — never directly to storage. The daemon owns a local, embedded graph database, a vector index, and a content-addressed store of extracted text, all encrypted at rest, all on-device by default. A hotkey or wake word opens a small overlay; your query gets embedded, matched against the vector index, expanded via graph traversal for connected context, and answered by a local LLM that always cites its sources. Full breakdown, including the exact IPC transports and storage layout, is in `ARCHITECTURE.md`.

```
you ──(hotkey / "Hey Vision")──▶ Query UI ──▶ Daemon ──▶ Vector search + Graph traversal ──▶ Local LLM ──▶ cited answer
                                                 │
file save / tab visit ──────────────────────────┘  (continuous background indexing)
```

## Tech Stack

- **Daemon:** Rust
- **Desktop shell:** Tauri
- **Graph DB:** embedded (Kùzu or Memgraph — final call tracked in `ARCHITECTURE.md` §9 / `TASKS.md` M0)
- **Vector index:** embedded (LanceDB or equivalent)
- **Local inference:** quantized LLM via `llama.cpp`/ONNX Runtime, on-device by default
- **IPC:** gRPC over Unix domain sockets (macOS/Linux) / named pipes (Windows) — never a network-exposed port

## Design Principles

1. **Local-first.** Everything required to index and query runs on-device; the network is never in the critical path.
2. **One writer.** A single daemon owns all state — no client, including the UI, ever writes to storage directly.
3. **No answer without a source.** Every synthesized response cites the document, timestamp, and path it came from.
4. **Non-intrusive.** The UI appears when summoned and disappears cleanly — this is a tool you invoke, not a dashboard you live in.

Full rationale for each of these lives in `ARCHITECTURE.md` §1 and `UI.SPEC.md` §1.

## Current Status

Following `ROADMAP.md`'s phase plan, M0 and M1's transport/persistence layer are done; M2 through M7 (retrieval, no LLM synthesis yet) are functionally working end to end, driven through a REPL client rather than the Tray App/Query UI, which haven't been built yet. `TASKS.md` §2 has the exact granular state milestone by milestone — what's real, what's an interim stand-in, and what's still missing.

**What actually runs today:** `cargo run -p vision-daemon` starts the daemon; `cargo run -p vision-daemon --example repl` in a second terminal drives it — grant a real folder, let the watcher (or an explicit `IngestEvent`) index a real file, then query it back and get a real ranked, cited snippet, not a fixed stub. Permissions and audit history persist across daemon restarts (`config.sqlite`/`audit.sqlite`/`graph.sqlite`/`vectors.sqlite` under `%LOCALAPPDATA%\Vision\` on Windows).

**What's an interim stand-in, not the real thing yet:** the graph DB and vector index are SQLite tables, not Kùzu/LanceDB; the embedding model is a deterministic hashing vectorizer (lexical matching), not a local neural model. `ARCHITECTURE.md` §9.1a and `TASKS.md` §4 spell out exactly what each stand-in is missing and what swapping in the real thing needs. There's no Tray App, no Query UI, no answer synthesis (M8) yet, and macOS/Linux are unverified (Windows-only transport + watcher testing so far).

## Contributing / Working on This

- Every change should trace back to a task in `TASKS.md`; if the work you want to do isn't there, add it to `TASKS.md` §3 or §4 before starting, don't just start.
- Nothing gets checked off a task without clearing the Excellence Bar in `TASKS.md` §1 — clean build, real tests, manually exercised, docs updated if reality diverged from what they said.
- If an implementation detail contradicts `ARCHITECTURE.md` or `UI.SPEC.md`, fix the doc in the same change — they're meant to stay true, not become stale aspiration documents.

## License

TBD.
