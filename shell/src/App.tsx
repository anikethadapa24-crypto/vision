import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import "./App.css";

/** Mirrors src-tauri/src/dto.rs's AnswerChunkDto. */
interface SourceRef {
  document_id: string;
  path: string;
  timestamp_unix_ms: number;
}
interface AnswerChunk {
  token: string;
  is_final: boolean;
  sources: SourceRef[];
}

type QueryState = "idle" | "thinking" | "streaming" | "answered" | "error";
type ErrorKind = "unreachable" | "no-results";

const STILL_SEARCHING_DELAY_MS = 2000;

/**
 * Floating Query UI (docs/UI.SPEC.md §3/§4). Backed by the real `Query` RPC
 * (docs/TASKS.md M7) via the `submit_query` command — right now that means
 * real ranked, cited snippets, not a synthesized answer yet (that's M8,
 * next in docs/TASKS.md). Rendering already treats each chunk's `token` as
 * an appended text segment, which is also exactly the right shape for
 * streamed prose once synthesis lands — this component doesn't change
 * shape when that happens, only what's inside each chunk.
 */
function App() {
  const inputRef = useRef<HTMLInputElement>(null);
  // Tracks whether any chunk with real content has arrived for the
  // in-flight query. A ref rather than state because "query-done"'s
  // handler needs to read it synchronously without depending on this
  // effect (which only runs once) re-subscribing to fresh closures.
  const hasContentRef = useRef(false);
  const [query, setQuery] = useState("");
  const [state, setState] = useState<QueryState>("idle");
  const [answer, setAnswer] = useState("");
  const [sources, setSources] = useState<SourceRef[]>([]);
  const [errorKind, setErrorKind] = useState<ErrorKind>("unreachable");
  const [stillSearching, setStillSearching] = useState(false);
  const [copyLabel, setCopyLabel] = useState("Copy");

  useEffect(() => {
    const win = getCurrentWindow();

    const unlistenFocus = win.listen("tauri://focus", () => {
      inputRef.current?.focus();
    });

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        void win.hide();
      }
    }
    document.addEventListener("keydown", onKeyDown);

    const unlistenChunk = listen<AnswerChunk>("query-chunk", (event) => {
      const chunk = event.payload;
      setState("streaming");
      if (chunk.token) {
        hasContentRef.current = true;
        setAnswer((prev) => (prev ? `${prev}\n\n${chunk.token}` : chunk.token));
      }
      if (chunk.sources.length > 0) {
        hasContentRef.current = true;
        setSources((prev) => [...prev, ...chunk.sources]);
      }
    });

    const unlistenDone = listen("query-done", () => {
      // A stream that finished with no content and no citations means the
      // query genuinely matched nothing — that's the no-results Error
      // variant (docs/UI.SPEC.md §5a), not a completed answer.
      if (hasContentRef.current) {
        setState("answered");
      } else {
        setErrorKind("no-results");
        setState("error");
      }
    });

    const unlistenError = listen<string>("query-error", () => {
      setErrorKind("unreachable");
      setState("error");
    });

    return () => {
      document.removeEventListener("keydown", onKeyDown);
      void unlistenFocus.then((f) => f());
      void unlistenChunk.then((f) => f());
      void unlistenDone.then((f) => f());
      void unlistenError.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (state !== "thinking") {
      setStillSearching(false);
      return;
    }
    const timer = setTimeout(() => setStillSearching(true), STILL_SEARCHING_DELAY_MS);
    return () => clearTimeout(timer);
  }, [state]);

  function submit() {
    const text = query.trim();
    if (!text || state === "thinking" || state === "streaming") return;

    hasContentRef.current = false;
    setAnswer("");
    setSources([]);
    setState("thinking");
    void invoke("submit_query", { text }).catch(() => {
      // query-error event already covers user-facing state; this just
      // stops an unhandled promise rejection in the console.
    });
  }

  function onKeyDownInput(event: ReactKeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") submit();
  }

  function copyAnswer() {
    void navigator.clipboard.writeText(answer).then(() => {
      setCopyLabel("Copied");
      setTimeout(() => setCopyLabel("Copy"), 1200);
    });
  }

  const thinking = state === "thinking";
  const showAnswerArea = state === "streaming" || state === "answered" || state === "error";

  return (
    <div className="vision-root">
      <div className={`query-pill${thinking ? " query-pill--thinking" : ""}`}>
        <span className="search-icon" aria-hidden="true">
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
            <circle cx="6.8" cy="6.8" r="4.8" stroke="currentColor" strokeWidth="1.5" />
            <path d="M10.5 10.5L14 14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          </svg>
        </span>
        <input
          ref={inputRef}
          className="query-input"
          type="text"
          placeholder="Ask Vision…"
          autoFocus
          disabled={thinking}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDownInput}
        />
        <span className="mic-icon" aria-hidden="true" title="Voice input — coming soon">
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none">
            <rect x="5.5" y="1.5" width="5" height="8" rx="2.5" stroke="currentColor" strokeWidth="1.3" />
            <path d="M3 8a5 5 0 0 0 10 0M8 13v2" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
          </svg>
        </span>
      </div>

      {thinking && (
        <div className="answer-area">
          <div className="answer-shimmer" />
          <div className="answer-shimmer" />
          {stillSearching && <div className="still-searching">still searching…</div>}
        </div>
      )}

      {showAnswerArea && state !== "error" && (
        <div className="answer-area">
          <div className="answer-body">{answer}</div>
          {sources.length > 0 && (
            <div className="source-chips">
              {sources.map((s, i) => (
                <button
                  className="source-chip"
                  key={`${s.document_id}-${i}`}
                  title={`Reveal ${s.path}`}
                  onClick={() => void revealItemInDir(s.path)}
                >
                  {s.path.split(/[\\/]/).pop()}
                </button>
              ))}
            </div>
          )}
          {state === "answered" && (
            <div className="action-row">
              <button className="action-button" onClick={copyAnswer}>
                {copyLabel}
              </button>
              <button className="action-button" disabled title="Graph Explorer — coming soon">
                View in graph
              </button>
              <button className="action-button" disabled title="Multi-turn — coming soon">
                Ask follow-up
              </button>
            </div>
          )}
        </div>
      )}

      {state === "error" && (
        <div className="answer-area">
          <div className="error-message">
            {errorKind === "unreachable"
              ? "Vision isn't running"
              : "Nothing found — try rephrasing"}
          </div>
          <div className="action-row">
            <button className="action-button" onClick={submit}>
              Retry
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
