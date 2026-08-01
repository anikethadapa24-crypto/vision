# UI.SPEC.md — Vision Desktop Front End

This is the front-end specification for Vision: every surface the app presents, the design tokens that back them, and the components they're built from. It governs implementation of the Tauri webview clients described in `ARCHITECTURE.md` §3 (Tray App, Floating Query UI, Settings, Graph Explorer). Build against this file rather than improvising per-screen — it keeps the app feeling like one system instead of eight prototypes stapled together.

## 1. Design Principles

1. **Non-intrusive by default.** Vision runs in the background; its UI should feel like a tool that appears when summoned and disappears cleanly, never a persistent window competing for desktop space (PRD §5.4 "Floating UI... lightweight, non-intrusive overlay").
2. **Keyboard/voice first, mouse second.** The primary interaction (hotkey → query → answer) must be fully operable without touching the mouse. Mouse-driven surfaces (Graph Explorer, Settings) are secondary and reached deliberately, not by default.
3. **Native, not branded-chrome.** Vision borrows each OS's window conventions (traffic lights on macOS, snap layouts on Windows) rather than a custom-chrome cross-platform look. Users should not feel like they're inside an Electron wrapper.
4. **Every answer shows its work.** Per PRD §5.5, no synthesized answer appears without visible source attribution in the same view — never a citation hidden behind a click.
5. **Calm, not clinical.** This is a memory tool, not an analytics dashboard — motion is subtle, density is generous, and the graph views (§5f) read as a map to explore, not a monitoring console.

## 2. Design Tokens

All tokens are CSS custom properties, scoped so light/dark swap in one place. Structure follows the same role-based pattern used for the Graph Explorer's data-viz layer (§5f) so the whole app shares one palette source of truth.

### 2.1 Chrome & ink

| Role | Token | Light | Dark |
|---|---|---|---|
| App surface | `--surface-1` | `#fcfcfb` | `#1a1a19` |
| Page plane / window background | `--plane` | `#f9f9f7` | `#0d0d0d` |
| Raised surface (cards, popovers) | `--surface-2` | `#ffffff` | `#232322` |
| Primary ink | `--text-primary` | `#0b0b0b` | `#ffffff` |
| Secondary ink | `--text-secondary` | `#52514e` | `#c3c2b7` |
| Muted ink (timestamps, hints) | `--text-muted` | `#898781` | `#898781` |
| Hairline / divider | `--border` | `rgba(11,11,11,0.10)` | `rgba(255,255,255,0.10)` |
| Gridline (Graph Explorer only) | `--gridline` | `#e1e0d9` | `#2c2c2a` |

```css
.vision-root {
  color-scheme: light;
  --surface-1: #fcfcfb; --plane: #f9f9f7; --surface-2: #ffffff;
  --text-primary: #0b0b0b; --text-secondary: #52514e; --text-muted: #898781;
  --border: rgba(11,11,11,0.10);
}
@media (prefers-color-scheme: dark) {
  :root:where(:not([data-theme="light"])) .vision-root {
    color-scheme: dark;
    --surface-1: #1a1a19; --plane: #0d0d0d; --surface-2: #232322;
    --text-primary: #ffffff; --text-secondary: #c3c2b7; --text-muted: #898781;
    --border: rgba(255,255,255,0.10);
  }
}
:root[data-theme="dark"] .vision-root { /* same values as the media block above */ }
```

The `data-theme` scope (viewer's in-app toggle) must win over the OS `prefers-color-scheme` setting in both directions — this is what lets a user force light/dark independent of their OS.

### 2.2 Status colors (fixed, never themed)

Used for indexing state, connection state, and permission state — never for arbitrary series or node types (that would let a status color impersonate content, which is the one thing PRD §8.3's guardrail language explicitly warns against for graph trust):

| State | Token | Hex | Used for |
|---|---|---|---|
| Good | `--status-good` | `#0ca30c` (light) / `#0ca30c` (dark) | Daemon connected, folder fully indexed, sync healthy |
| Warning | `--status-warning` | `#fab219` | Indexing throttled (resource pressure), permission partially granted |
| Serious | `--status-serious` | `#ec835a` | Sync degraded, model download stalled |
| Critical | `--status-critical` | `#d03b3b` | Daemon disconnected, extraction failure, permission revoked mid-index |

Every status indicator pairs its color with an icon and a text label (tray tooltip, settings row, toast) — color is never the only signal, per the accessibility rule this palette is built on.

### 2.3 Node-type identity system (Graph Explorer)

The concept map (PRD §5.6) is a node-link graph where any two node types can be adjacent on screen — the "all-pairs" case, not the "adjacent-series" case a bar chart gets. Under all-pairs comparison, only the **first three** slots of the categorical palette clear the colorblind-safe floor; a straight 8-color-per-node-type mapping (one hue per `Document/Project/Person/Concept/Decision/Research/Lesson/Intel` type) would fail for anyone with a color vision deficiency past the third type on screen together.

So node identity is **icon-first, color-second**:

- **Primary identity = shape/icon**, always visible, never color-dependent: each of the 8 node types gets a distinct glyph (document page, project folder, person silhouette, lightbulb/concept, decision fork, research flask, lesson bookmark, intel/flag). Icons render in `--text-primary`, not in a type color.
- **Secondary color = macro-category**, capped at 3 slots (the only count the palette validates all-pairs in both modes):

| Macro-category | Node types | Slot | Light | Dark |
|---|---|---|---|---|
| Knowledge | Document, Concept, Research, Lesson | 1 · blue | `#2a78d6` | `#3987e5` |
| Structure | Project, Decision, Intel | 2 · orange | `#eb6834` | `#d95926` |
| People | Person | 3 · aqua | `#1baf7a` | `#199e70` |

- Edge type (`cites`, `relates-to`, `authored-by`, `used-in`) is encoded by **line style** (solid / dashed / dotted / double), not color — keeping color budget entirely for the macro-category fill.
- If a future revision needs true per-type hue (e.g., a legend-driven filter that isolates one type at a time), it must re-run `scripts/validate_palette.js --pairs all` before shipping — do not eyeball it.

### 2.4 Typography

System sans throughout, no display or serif face: `system-ui, -apple-system, "Segoe UI", sans-serif`. Matches each OS's native text rendering rather than importing a brand font that looks foreign in a menu-bar-adjacent tool.

| Role | Size / weight | Used for |
|---|---|---|
| Display | 20px / 600 | Answer headline (first line of a synthesized answer, if the orchestrator emits one) |
| Body | 14px / 400 | Answer body, list rows, settings labels |
| Body emphasis | 14px / 600 | Node titles, source doc names |
| Caption | 12px / 400 | Timestamps, file paths, muted metadata |
| Label | 11px / 600, uppercase, +0.02em tracking | Section headers in Settings, group labels in Graph Explorer legend |

Numeric figures (timestamps, node/edge counts in Settings) use default proportional figures; only genuinely tabular data (Audit Log's timestamp column) uses `font-variant-numeric: tabular-nums`.

### 2.5 Spacing, radius, elevation

- **Spacing scale:** 4 / 8 / 12 / 16 / 24 / 32 / 48px. Query UI and list rows default to the 8/16 rhythm; Settings tabs use 24/32 for section breathing room.
- **Radius:** 8px for cards and input fields, 6px for buttons/chips, 999px (pill) for tags and the query input itself when idle.
- **Elevation:** the Floating Query UI is the only surface that floats above other applications, so it's the only one with a real shadow: `0 8px 32px rgba(0,0,0,0.24)` light, `0 8px 32px rgba(0,0,0,0.55)` dark, plus a 1px `--border` ring. In-window surfaces (Settings, Audit Log, Graph Explorer) use flat panels with hairline dividers, no shadow — they don't need to visually separate from the desktop the way the overlay does.

### 2.6 Motion

- **Overlay enter:** 120ms ease-out scale (0.98 → 1.0) + fade. Fast enough that hotkey-to-visible feels instant, not enough to feel like a "modal slam."
- **Overlay exit:** 80ms fade only, no scale — exiting should feel quieter than entering.
- **Streamed answer tokens:** append with no animation (no fade-per-token) — token-level motion on fast local generation reads as jittery, not "alive."
- **List/graph transitions:** 150ms ease-in-out for node selection, panel expand/collapse. Nothing in Vision animates for longer than 200ms; this is a retrieval tool, not a showcase.
- **Reduced motion:** respect `prefers-reduced-motion` — overlay enter/exit collapse to a straight opacity fade, streaming and graph transitions lose easing curves (become instant).

## 3. Surface Inventory

| Surface | Window type | Opens via | Closes via |
|---|---|---|---|
| System Tray Icon | OS tray/menu-bar item | Always present when daemon connected | N/A (persistent) |
| Floating Query UI | Borderless overlay, always-on-top | Hotkey, wake word, tray menu "Ask Vision" | `Esc`, click-outside, answer-dismiss |
| Onboarding Wizard | Standard window, modal on first run | First launch after install | Completing or skipping setup |
| Settings Window | Standard window | Tray menu "Settings", Query UI overflow menu | Standard window close |
| Audit Log | Tab within Settings Window | Settings → Privacy tab | — |
| Graph Explorer | Standard window (resizable, can go full-screen) | Tray menu "Explore", Query UI "View in graph" action on any result | Standard window close |
| Project Workspace | View within Graph Explorer, scoped to one project subgraph | Selecting a project node, or a project shortcut list | Back navigation within Graph Explorer |
| Timeline View | View within Graph Explorer, toggled per concept/project | Toggle from Project Workspace or a concept node's detail panel | Toggle back to graph view |
| Browser Extension Popup | Browser-native popup | Extension toolbar icon click | Click-outside (browser-managed) |

## 4. Interaction States — Floating Query UI

This is the surface used most, so its state machine is fully specified. All states share the same pill-shaped input at the top; only what's below it changes.

```
[Idle] --hotkey/wake word--> [Listening?] --typed or spoken query submitted--> [Thinking]
[Thinking] --first token arrives--> [Streaming]
[Streaming] --stream completes--> [Answered]
[Answered] --new query typed--> [Thinking] (answer area clears, new turn begins; §M15 conversation history persists across turns in-session)
[Thinking|Streaming] --error from daemon--> [Error]
[any state] --Esc / click-outside--> [Idle] (window hides, does not quit)
```

| State | Visual | Notes |
|---|---|---|
| **Idle** | Just the input pill, placeholder text `Ask Vision…`, subtle mic icon at the right edge | Appears centered in the upper third of the active display, per PRD's "floats on top of any application" |
| **Listening** (voice path only) | Input pill shows a live waveform in place of placeholder text | Only reachable via wake word or mic-icon click, not the default text path |
| **Thinking** | Input pill freezes with submitted query; a single-line skeleton shimmer appears below | Cap this state's visible duration expectation at the PRD §7.3 2s p90 query budget — if it runs longer, show a subtle "still searching…" caption after 2s rather than leaving a bare shimmer |
| **Streaming** | Answer text appends live below the input; a compact source-chip row builds incrementally as citations resolve | Chips are clickable immediately, even while later chips are still arriving |
| **Answered** | Full answer + source chip row + a slim action row (`Copy`, `View in graph`, `Ask follow-up`) | This is the resting state for a completed turn — nothing auto-dismisses it |
| **Error** | Answer area replaced by a single-line message + retry action, in `--status-critical` icon only (not full-red background — errors here are informational, not alarming) | Distinguish daemon-unreachable ("Vision isn't running") from no-results ("Nothing found — try rephrasing") as two different copy states, not one generic error |

**Multi-turn layout (M15):** each turn stacks as a compact card (query line + collapsed answer summary); only the latest turn is expanded by default. Older turns collapse to a single line, expandable on click — the overlay must not grow unbounded during a long conversation.

## 5. Surface Specs

### 5a. System Tray Icon & Menu

- Icon states: default (daemon healthy), throttled (small `--status-warning` dot), error (small `--status-critical` dot), indexing-in-progress (subtle pulsing ring, respects reduced-motion by becoming a static dot instead).
- Menu (top to bottom): `Ask Vision` (opens Query UI), `Explore Graph`, separator, indexing status line (e.g. "12,403 items indexed" — plain text, not clickable), `Settings…`, separator, `Quit Vision`.
- Right-click (Windows/Linux) and click (macOS) both open the same menu — no hidden secondary menu.

### 5b. Onboarding Wizard

Steps, one per screen, linear with a progress dots indicator (no skipping ahead, back always allowed):

1. **Welcome** — one-line value prop, `Get Started`.
2. **Folder permissions** — folder picker list (PRD §5.7 granular permissions), each row a path + toggle, defaults to none selected (opt-in, never pre-checked).
3. **Browser & app scopes** — checkboxes for detected browsers/apps, same opt-in-by-default rule.
4. **Model download** — progress bar tied to real Model Cache download (`ARCHITECTURE.md` §5.2), with a size/time estimate; this step cannot be skipped since nothing works without it, but it can run in the background while the user finishes remaining steps.
5. **Hotkey confirmation** — shows the default global hotkey, lets the user rebind before finishing.
6. **Done** — confirms daemon is running, offers `Ask your first question` which opens the Query UI directly.

### 5c. Settings Window

Tabbed, left-side vertical tab list (native list style per OS):

- **General** — hotkey rebinding, wake-word on/off, launch-at-login toggle, theme (System/Light/Dark).
- **Permissions** — same folder/app/browser list as onboarding, editable any time; revoking a folder here triggers the coordinated-delete flow from `ARCHITECTURE.md` §5.3 with a confirmation dialog naming what will be removed.
- **Privacy** → **Audit Log** (5d).
- **Models** — which local models are active, storage used, re-download/update actions.
- **Sync** (Phase 2, premium-gated) — enable/disable cloud sync, last sync time, device list.
- **About** — version, daemon connection diagnostics.

### 5d. Audit Log

- Flat, filterable, sortable-by-time list. Columns: source icon+type, path/URL, indexed timestamp (tabular-nums), graph node link.
- Row-level `Delete` action and a header-level date-range delete (PRD §5.7 "delete specific items or entire time ranges") — the date-range control follows the standard preset-list pattern (today / 7d / 30d / custom) rather than a bespoke calendar widget.
- Every delete action shows a confirmation naming the count of items affected before committing (coordinated delete is irreversible per `ARCHITECTURE.md` §5.3).

### 5e. Graph Explorer

- Canvas-based force-directed or hierarchical layout (implementation detail for engineering, not this spec) rendering nodes/edges per the token system in §2.3.
- **Legend is always present** whenever more than one macro-category is on screen (per the palette's rule that ≥2 series always carries a legend) — shown as a compact strip along the bottom, listing the 3 macro-categories plus the 8 icons, not repeating color+icon redundantly beyond what's needed to read the graph.
- Click a node → detail panel slides in from the right: title, type, metadata (status/timestamp/tags per PRD §5.3), direct edges, `Open source`, `View timeline` (if applicable).
- Search-within-graph input pinned to the top, separate from the global Query UI — this searches/filters the *visible* graph, it does not invoke the LLM.
- A **table view toggle** exists (per the accessibility rule that any chart-like view ships a non-visual fallback) — same node/edge data as a sortable, filterable table.

### 5f. Project Workspace

- Enter by clicking a `Project`-type node; view scopes the Graph Explorer canvas to that project's subgraph only (no visual redesign, it's a filtered Graph Explorer state).
- Header shows project name, item count, last-activity timestamp, and a `View Timeline` toggle (5g).

### 5g. Timeline View

- Horizontal timeline, one row per concept/project, nodes plotted by their timestamp/version history (PRD §5.6).
- This is a sequential/ordinal encoding, not categorical — uses the single blue sequential ramp (§2.1 sequential hue, `references/palette.md` ordinal-clamped range) rather than the macro-category colors, since here the encoding is "how recent," not "what kind."

### 5h. Browser Extension Popup

- Minimal: connection status to daemon (good/warning/critical dot + label), a toggle for "index this site," and a link to open full Settings — the popup does not duplicate the Query UI; querying always happens through the desktop overlay.

## 6. Component Inventory

| Component | Used in | Key states |
|---|---|---|
| Query Input (pill) | Query UI | idle, focused, submitted/frozen, listening (waveform) |
| Answer Card | Query UI | streaming, complete, error |
| Source Citation Chip | Query UI, Graph Explorer detail panel | default, hover (preview tooltip), clicked (opens source) |
| Result/List Row | Audit Log, Settings lists, collapsed conversation turns | default, hover, selected, disabled (e.g. permission revoked) |
| Toggle Row | Settings, Onboarding | on, off, disabled-with-reason |
| Permission Row | Onboarding, Settings → Permissions | granted, partial (folder exists but subpath excluded), revoked |
| Graph Node | Graph Explorer | default, hover, selected, dimmed (when filtered out) |
| Graph Edge | Graph Explorer | default, highlighted (connected to selected node), dimmed |
| Type Badge (icon + label) | Graph Explorer detail panel, Audit Log rows | one per node type (§2.3) |
| Status Dot | Tray icon, Extension popup, Settings → About | good, warning, serious, critical |
| Empty State | Audit Log (nothing indexed yet), Graph Explorer (no results for filter), Query UI (no matches) | always icon + one-line message + one suggested action, never a bare "No results" |
| Loading Skeleton | Query UI "Thinking" state, Graph Explorer initial load | shimmer, respects reduced-motion (becomes static gray block) |
| Toast/Notification | Settings save confirmations, sync status changes | auto-dismiss 4s, status-colored icon only, never a colored background |

## 7. Keyboard Shortcuts

| Shortcut | Action | Scope |
|---|---|---|
| Global hotkey (default `Ctrl+Shift+V` / `Cmd+Shift+V`) | Open Query UI | System-wide |
| `Esc` | Dismiss current overlay/panel | Query UI, Graph Explorer detail panel |
| `Enter` | Submit query | Query UI input focused |
| `Cmd/Ctrl+K` | Focus search-within-graph | Graph Explorer |
| `Up/Down` | Navigate collapsed conversation turns | Query UI, multi-turn state |
| `Cmd/Ctrl+,` | Open Settings | Anywhere in-app |

## 8. Accessibility Requirements

- Every status/identity signal pairs color with an icon and/or text label — nowhere in the app is color the sole carrier of meaning (§2.2, §2.3).
- Graph Explorer ships a table-view fallback (§5e) as the non-visual equivalent of the canvas.
- `prefers-reduced-motion` is respected app-wide (§2.6); no exceptions.
- Full keyboard operability for the primary query loop (§7); Graph Explorer navigation should support arrow-key node traversal in a future pass — tracked as an open item below, not blocking Phase 1.
- Contrast: body text against `--surface-1`/`--plane` meets WCAG AA in both themes (inherits from the validated palette's ink tokens, §2.1); status colors used at icon-size are exempt from text-contrast minimums per the palette's documented icon+label mitigation (`references/palette.md` Status palette notes), but must never be the only cue.

## 9. Platform-Specific Conventions

| Concern | Windows | macOS | Linux |
|---|---|---|---|
| Tray icon host | System tray (notification area) | Menu bar | AppIndicator/StatusNotifier (DE-dependent) |
| Window controls | Top-right, native | Top-left traffic lights | Follows DE convention (GNOME/KDE) |
| Global hotkey registration | Win32 `RegisterHotKey` | Carbon/Cocoa event tap | DE-specific; document as best-effort, not guaranteed |
| Overlay always-on-top behavior | `WS_EX_TOPMOST` | `NSWindow` floating level | Varies by compositor; degrade gracefully to non-topmost if unsupported |

Do not build one cross-platform tray/menu abstraction that lowest-common-denominators all three — prefer Tauri's native bindings per OS and accept minor menu-shape differences over a worse shared UI.

## 10. Open Items

- Arrow-key node traversal in Graph Explorer (accessibility follow-up, not Phase 1 blocking).
- Whether the Timeline View (§5g) needs a diverging color for "before/after a decision point" — deferred until Project Workspace ships and we see real usage patterns (would use the blue↔red diverging pair from the palette if needed).
- Exact wording/tone for empty-state and error copy — needs a content pass once M7/M8 (`ROADMAP.md`) are functional and we have real failure cases to write copy against, rather than guessing them now.
