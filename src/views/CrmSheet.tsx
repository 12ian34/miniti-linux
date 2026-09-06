import { useEffect, useState } from "react";
import { crmPreview, crmSearch, crmSend, crmStatus, errorMessage } from "../api";
import type { CrmProvider, CrmRecord, CrmTask } from "../types";
import { Sheet } from "./MeetingTools";

/**
 * Shared CRM send sheet (Attio / Twenty): search a record, preview the note
 * payload, choose which action items become tasks, send. Fixed primary footer.
 */
export function CrmSheet({ meetingId, onClose }: { meetingId: string; onClose: () => void }) {
  const [provider, setProvider] = useState<CrmProvider>("attio");
  const [connected, setConnected] = useState<Record<CrmProvider, boolean | null>>({ attio: null, twenty: null });
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<CrmRecord[]>([]);
  const [selected, setSelected] = useState<CrmRecord | null>(null);
  const [tasks, setTasks] = useState<(CrmTask & { on: boolean })[]>([]);
  const [payload, setPayload] = useState<Record<string, unknown> | null>(null);
  const [showPayload, setShowPayload] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);

  useEffect(() => {
    (["attio", "twenty"] as CrmProvider[]).forEach((p) =>
      crmStatus(p)
        .then((s) => setConnected((c) => ({ ...c, [p]: s.connected })))
        .catch(() => setConnected((c) => ({ ...c, [p]: false }))),
    );
    crmPreview(meetingId)
      .then((p) => {
        setPayload(p.payload as Record<string, unknown>);
        setTasks(p.tasks.map((t) => ({ ...t, on: true })));
      })
      .catch((e) => setError(errorMessage(e)));
  }, [meetingId]);

  useEffect(() => {
    if (query.trim().length < 2) {
      setResults([]);
      return;
    }
    const t = window.setTimeout(() => {
      crmSearch(provider, query)
        .then(setResults)
        .catch((e) => setError(errorMessage(e)));
    }, 300);
    return () => window.clearTimeout(t);
  }, [query, provider]);

  async function send() {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      const r = await crmSend(
        provider,
        meetingId,
        selected.object_slug,
        selected.id.record_id,
        tasks.filter((t) => t.on).map(({ content, deadline_at }) => ({ content, deadline_at })),
      );
      const n = (r as { tasks_created?: number }).tasks_created ?? 0;
      setResult(`Sent to ${provider === "attio" ? "Attio" : "Twenty"}${n ? ` with ${n} task${n === 1 ? "" : "s"}` : ""}.`);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const isConnected = connected[provider];

  return (
    <Sheet title="send to CRM" onClose={onClose}>
      <div className="tab-strip inline">
        {(["attio", "twenty"] as CrmProvider[]).map((p) => (
          <button key={p} className={`tab ${provider === p ? "active" : ""}`} onClick={() => (setProvider(p), setSelected(null), setResults([]))}>
            {p}{connected[p] === false ? " (not connected)" : ""}
          </button>
        ))}
      </div>
      {isConnected === false && (
        <p className="muted">Connect {provider === "attio" ? "Attio" : "Twenty"} in Settings → Integrations first.</p>
      )}
      <input
        className="input"
        placeholder={`search ${provider === "twenty" ? "people, companies, opportunities" : "people, companies"}`}
        value={query}
        onChange={(e) => setQuery(e.currentTarget.value)}
        disabled={isConnected === false}
      />
      {results.length > 0 && !selected && (
        <div className="crm-results">
          {results.map((r) => (
            <button key={r.id.record_id} className="crm-row" onClick={() => setSelected(r)}>
              <span className="pill">{r.object_slug}</span>
              <span>{r.record_text}</span>
              <span className="muted small">{r.record_email ?? r.record_domain ?? r.record_detail ?? ""}</span>
            </button>
          ))}
        </div>
      )}
      {selected && (
        <div className="crm-selected">
          <span className="pill">{selected.object_slug}</span> {selected.record_text}
          <button className="ghost tiny" onClick={() => setSelected(null)}>change</button>
        </div>
      )}
      {tasks.length > 0 && (
        <div>
          <div className="panel-title">tasks from action items</div>
          {tasks.map((t, i) => (
            <label className="toggle" key={i}>
              <input type="checkbox" checked={t.on} onChange={(e) => setTasks((ts) => ts.map((x, j) => (j === i ? { ...x, on: e.currentTarget.checked } : x)))} />
              <span>{t.content}</span>
              {provider === "attio" && (
                <input
                  className="input date"
                  type="date"
                  value={t.deadline_at ?? ""}
                  onChange={(e) => setTasks((ts) => ts.map((x, j) => (j === i ? { ...x, deadline_at: e.currentTarget.value || null } : x)))}
                />
              )}
            </label>
          ))}
          <p className="muted tiny">
            {provider === "attio" ? "Tasks are assigned to the workspace member who connected Attio." : "Twenty tasks use workspace defaults (no deadline)."} Unchecked items stay in the note.
          </p>
        </div>
      )}
      <details open={showPayload} onToggle={(e) => setShowPayload((e.currentTarget as HTMLDetailsElement).open)}>
        <summary className="muted small">note contents</summary>
        <pre className="payload">{JSON.stringify(payload, null, 2)}</pre>
      </details>
      {error && <div className="banner error">{error}</div>}
      {result && <div className="banner ok">{result}</div>}
      <div className="save-row sheet-footer">
        <button className="btn primary" onClick={send} disabled={busy || !selected || isConnected === false}>
          {busy ? "sending…" : `send to ${provider}`}
        </button>
        <button className="btn secondary" onClick={onClose}>close</button>
      </div>
    </Sheet>
  );
}
