import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { catchUp, errorMessage, investigate } from "../api";
import type { CatchUp, Investigation, InvestigationScope } from "../types";

/** "catch me up" one-shot sheet (macOS Zoned Out). */
export function CatchUpButton({ meetingId }: { meetingId: string }) {
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<CatchUp | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function run() {
    setOpen(true);
    setBusy(true);
    setError(null);
    try {
      setResult(await catchUp(meetingId));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <button className="control" onClick={run} disabled={busy} title="Summarize the last few minutes">
        <span className="glyph">↻</span> catch me up
      </button>
      {open && (
        <Sheet title="catch me up" onClose={() => setOpen(false)}>
          {busy && <p className="muted">Reading the last few minutes…</p>}
          {error && <div className="banner error">{error}</div>}
          {result && (
            <>
              {result.current_topic && (
                <>
                  <div className="insight-title" style={{ color: "var(--insight-summary)" }}>right now</div>
                  <p>{result.current_topic}</p>
                </>
              )}
              {result.questions_for_you.length > 0 && (
                <>
                  <div className="insight-title" style={{ color: "var(--amber)" }}>questions for you</div>
                  <ul>{result.questions_for_you.map((q, i) => <li key={i}>{q}</li>)}</ul>
                </>
              )}
              {result.recent_discussion.length > 0 && (
                <>
                  <div className="insight-title" style={{ color: "var(--insight-discussion)" }}>recent discussion</div>
                  <ul>{result.recent_discussion.map((q, i) => <li key={i}>{q}</li>)}</ul>
                </>
              )}
              {result.key_decisions.length > 0 && (
                <>
                  <div className="insight-title" style={{ color: "var(--insight-actions)" }}>decisions</div>
                  <ul>{result.key_decisions.map((q, i) => <li key={i}>{q}</li>)}</ul>
                </>
              )}
              {!result.current_topic && result.recent_discussion.length === 0 && (
                <p className="muted">Nothing substantive in the recent window yet.</p>
              )}
            </>
          )}
          <div className="save-row">
            <button className="control" onClick={run} disabled={busy}>refresh</button>
            <button className="control primary" onClick={() => setOpen(false)}>done</button>
          </div>
        </Sheet>
      )}
    </>
  );
}

/** Explicit investigation: Web (OpenAI web search) or Codebase (bounded excerpts). */
export function InvestigateButton({
  meetingId,
  suggestedFocus,
  onDismissSuggestion,
  hasCodebaseRoot,
}: {
  meetingId: string;
  suggestedFocus: string | null;
  onDismissSuggestion: () => void;
  hasCodebaseRoot: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [focus, setFocus] = useState("");
  const [scope, setScope] = useState<InvestigationScope>("web");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<Investigation | null>(null);
  const [error, setError] = useState<string | null>(null);

  function openWith(initial: string) {
    setFocus(initial);
    setResult(null);
    setError(null);
    setOpen(true);
  }

  async function run() {
    if (!focus.trim()) return;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      setResult(await investigate(meetingId, scope, focus.trim()));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <button className="control" onClick={() => openWith(suggestedFocus ?? "")} title="Research a question from the meeting">
        <span className="glyph">⌕</span> investigate
      </button>
      {suggestedFocus && !open && (
        <div className="banner warn suggestion">
          <span className="ellipsis">Worth investigating? “{suggestedFocus}”</span>
          <button className="ghost" onClick={() => openWith(suggestedFocus)}>investigate</button>
          <button className="ghost" onClick={onDismissSuggestion}>×</button>
        </div>
      )}
      {open && (
        <Sheet title="investigate" onClose={() => setOpen(false)}>
          <textarea
            className="notes-area short"
            placeholder="What should miniti look into?"
            value={focus}
            onChange={(e) => setFocus(e.currentTarget.value)}
            maxLength={1000}
          />
          <div className="tab-strip inline">
            <button className={`tab ${scope === "web" ? "active" : ""}`} onClick={() => setScope("web")}>web</button>
            <button
              className={`tab ${scope === "codebase" ? "active" : ""}`}
              onClick={() => setScope("codebase")}
              title={hasCodebaseRoot ? "" : "Choose a codebase folder in Settings first"}
            >
              codebase{hasCodebaseRoot ? "" : " (set folder)"}
            </button>
          </div>
          <div className="save-row">
            <button className="control primary" onClick={run} disabled={busy || !focus.trim()}>
              {busy ? "investigating…" : "run"}
            </button>
            <button className="control" onClick={() => setOpen(false)}>close</button>
          </div>
          {error && <div className="banner error">{error}</div>}
          {result && (
            <div className="investigation">
              <p>{result.answer}</p>
              {result.sources?.length > 0 && (
                <>
                  <div className="insight-title">sources</div>
                  {result.sources.map((s, i) => (
                    <a key={i} href="#" className="citation" onClick={(e) => (e.preventDefault(), openUrl(s.url))}>
                      {s.title}
                    </a>
                  ))}
                </>
              )}
              {result.referenced_files?.length > 0 && (
                <p className="muted tiny">files reviewed: {result.referenced_files.join(", ")}</p>
              )}
            </div>
          )}
        </Sheet>
      )}
    </>
  );
}

export function Sheet({ title, onClose, children }: { title: string; onClose: () => void; children: React.ReactNode }) {
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" onClick={(e) => e.stopPropagation()}>
        <div className="sheet-head">
          <span className="panel-title">{title}</span>
          <button className="ghost" onClick={onClose}>×</button>
        </div>
        <div className="sheet-body">{children}</div>
      </div>
    </div>
  );
}
