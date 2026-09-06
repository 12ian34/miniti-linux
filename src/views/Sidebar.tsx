import { useEffect, useMemo, useRef, useState } from "react";
import { errorMessage, hasBridge, importGranolaCsv, insightsFinishing, onInsightsStatus, setPinned } from "../api";
import { useTauriEvent } from "../useEvent";
import { displayTitle, duration, groupMeetings, timeOnly } from "../format";
import { useStore } from "../store";
import type { Meeting } from "../types";

const LANG_FLAG: Record<string, string> = {
  es: "🇪🇸", sv: "🇸🇪", el: "🇬🇷", fr: "🇫🇷", de: "🇩🇪", pt: "🇵🇹", it: "🇮🇹", nl: "🇳🇱", pl: "🇵🇱", ru: "🇷🇺",
};

/** Port of the macOS `TerminalSidebar`: 220 px expanded, 52 px collapsed rail. */
export function Sidebar() {
  const store = useStore();
  const { route, navigate, meetings, sidebarOpen, setSidebarOpen, recording, refreshMeetings } = store;
  const [query, setQuery] = useState("");
  const [historyOpen, setHistoryOpen] = useState(true);
  const [finishing, setFinishing] = useState<string[]>([]);
  const [importMsg, setImportMsg] = useState<string | null>(null);
  const [focusIdx, setFocusIdx] = useState(-1);
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!hasBridge) return;
    insightsFinishing().then(setFinishing).catch(() => {});
  }, [meetings]);
  useTauriEvent(onInsightsStatus, () => {
    insightsFinishing().then(setFinishing).catch(() => {});
  });

  const current = recording.recording ? meetings.find((m) => m.id === recording.meeting_id) : undefined;
  const historical = useMemo(() => meetings.filter((m) => m.id !== current?.id), [meetings, current]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return historical;
    return historical.filter(
      (m) =>
        displayTitle(m).toLowerCase().includes(q) ||
        m.summary.toLowerCase().includes(q) ||
        m.notes.toLowerCase().includes(q) ||
        m.topics.toLowerCase().includes(q),
    );
  }, [historical, query]);
  const groups = useMemo(
    () => (query.trim() ? [{ label: "results", meetings: filtered }] : groupMeetings(filtered)),
    [filtered, query],
  );
  const flat = useMemo(() => groups.flatMap((g) => g.meetings), [groups]);
  const selectedId = route.kind === "meeting" ? route.id : null;

  // "/" focuses search, ↑K / ↓J move, Enter opens (macOS keyboard contract).
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const t = e.target as HTMLElement | null;
      const editing = !!t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA");
      if (e.key === "/" && !editing && sidebarOpen) {
        e.preventDefault();
        setHistoryOpen(true);
        searchRef.current?.focus();
        return;
      }
      if (editing && t !== searchRef.current) return;
      if (!sidebarOpen) return;
      if ((e.key === "ArrowDown" || (e.key.toLowerCase() === "j" && !editing)) && flat.length) {
        e.preventDefault();
        setFocusIdx((i) => Math.min(flat.length - 1, i + 1));
      } else if ((e.key === "ArrowUp" || (e.key.toLowerCase() === "k" && !editing)) && flat.length) {
        e.preventDefault();
        setFocusIdx((i) => Math.max(0, i - 1));
      } else if (e.key === "Enter" && focusIdx >= 0 && flat[focusIdx]) {
        e.preventDefault();
        navigate({ kind: "meeting", id: flat[focusIdx].id });
      } else if (e.key === "Escape" && t === searchRef.current) {
        setQuery("");
        searchRef.current?.blur();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [sidebarOpen, flat, focusIdx, navigate]);

  async function togglePin(e: React.MouseEvent, m: Meeting) {
    e.stopPropagation();
    await setPinned(m.id, !m.pinned);
    await refreshMeetings();
  }
  async function importGranola() {
    setImportMsg(null);
    try {
      const r = await importGranolaCsv();
      if (!r) return;
      const parts = [r.imported > 0 ? `imported ${r.imported}` : "nothing new"];
      if (r.duplicates > 0) parts.push(`${r.duplicates} already imported`);
      if (r.skipped_rows > 0) parts.push(`${r.skipped_rows} skipped`);
      setImportMsg(parts.join(" · "));
      await refreshMeetings();
    } catch (e) {
      setImportMsg(errorMessage(e));
    }
  }

  if (!sidebarOpen) {
    return (
      <aside className="rail">
        <button className="rail-logo" title="Home" onClick={() => navigate({ kind: "home" })}>⬢</button>
        <div className="gdiv" />
        <div className="rail-nav">
          <button
            className={`rail-btn ${route.kind === "home" || (current && selectedId === current.id) ? "active" : ""}`}
            title={current ? "Current meeting" : "Home"}
            onClick={() => navigate(current ? { kind: "meeting", id: current.id } : { kind: "home" })}
          >
            {current ? <span className={`dot ${recording.recording ? "dot-rec pulse" : "dot-info"}`} /> : <IconHome />}
          </button>
          <button className={`rail-btn ${route.kind === "coaching" ? "active" : ""}`} title="Coaching" onClick={() => navigate({ kind: "coaching" })}>
            <IconChart />
          </button>
          {historical.length > 0 && (
            <button className={`rail-btn ${selectedId && selectedId !== current?.id ? "active" : ""}`} title="History" onClick={() => { setHistoryOpen(true); setSidebarOpen(true); }}>
              <IconClock />
            </button>
          )}
        </div>
        <div className="rail-spacer" />
        <div className="gdiv" />
        <button className="rail-btn boxed" title="Show sidebar (Ctrl+[)" onClick={() => setSidebarOpen(true)}><IconSidebar /></button>
      </aside>
    );
  }

  return (
    <aside className="sidebar">
      <button className="brand" onClick={() => navigate({ kind: "home" })}>
        <span className="hex">⬢</span>
        <span className="brand-mark">miniti</span>
      </button>
      <div className="gdiv" />

      <nav className="side-nav">
        {current ? (
          <button className={`side-item ${selectedId === current.id ? "active" : ""}`} onClick={() => navigate({ kind: "meeting", id: current.id })}>
            <span className={`dot ${recording.recording ? "dot-rec pulse" : "dot-info"}`} />
            <span className="ellipsis">{displayTitle(current)}</span>
            <span className="kbd">rec</span>
          </button>
        ) : (
          <button className={`side-item ${route.kind === "home" ? "active" : ""}`} onClick={() => navigate({ kind: "home" })}>
            <IconHome /><span>home</span><span className="kbd">Ctrl+N</span>
          </button>
        )}
        <button className={`side-item ${route.kind === "coaching" ? "active" : ""}`} onClick={() => navigate({ kind: "coaching" })}>
          <IconChart /><span>coaching</span>
        </button>
        {historical.length > 0 && (
          <button className={`side-item ${selectedId && selectedId !== current?.id ? "active" : ""}`} onClick={() => setHistoryOpen(!historyOpen)}>
            <IconClock /><span>history</span>
            <span className="side-trailing">{historical.length} <span className="chev">{historyOpen ? "▾" : "▸"}</span></span>
          </button>
        )}
      </nav>

      {historical.length > 0 && historyOpen && (
        <>
          <div className="kbd-hints"><span className="kbd">/</span><span className="kbd">↑K</span><span className="kbd">↓J</span></div>
          <label className={`search-field ${query ? "on" : ""}`}>
            <span className="slash">/</span>
            <input ref={searchRef} placeholder="search meetings" value={query} onChange={(e) => { setQuery(e.currentTarget.value); setFocusIdx(-1); }} />
            {query && <span className="count">{filtered.length}</span>}
            {query && <button className="ghost tiny" onClick={() => setQuery("")}>×</button>}
          </label>
          <div className="side-list">
            {groups.length === 0 && <p className="muted tiny pad">no meetings match</p>}
            {groups.map((g) => (
              <div key={g.label} className="side-group">
                <div className="side-group-label">{g.label}</div>
                {g.meetings.map((m) => {
                  const idx = flat.indexOf(m);
                  const lang = m.language !== "en" ? LANG_FLAG[m.language] : null;
                  return (
                    <div
                      key={m.id}
                      className={`side-row ${selectedId === m.id ? "active" : ""} ${focusIdx === idx ? "focus" : ""}`}
                      onClick={() => navigate({ kind: "meeting", id: m.id })}
                    >
                      {m.pinned && <div className="pinned-label">pinned</div>}
                      <div className="side-row-title ellipsis">{displayTitle(m)}</div>
                      <div className="side-row-meta">
                        <span>{timeOnly(m.started_at)}</span>
                        {duration(m) && <><span className="sep">•</span><span>{duration(m)}</span></>}
                        {lang && <><span className="sep">•</span><span>{lang} {m.language.toUpperCase()}</span></>}
                        {m.import_source && <><span className="sep">•</span><span>{m.import_source.split(":")[0]}</span></>}
                      </div>
                      {finishing.includes(m.id) && <div className="finishing"><span className="dot dot-info pulse" /> finishing insights…</div>}
                      <div className="row-actions">
                        <button className="ghost tiny" title={m.pinned ? "unpin" : "pin"} onClick={(e) => togglePin(e, m)}>{m.pinned ? "unpin" : "pin"}</button>
                      </div>
                    </div>
                  );
                })}
              </div>
            ))}
          </div>
        </>
      )}
      {(historical.length === 0 || !historyOpen) && <div className="side-spacer" />}

      {importMsg && <p className="muted tiny pad">{importMsg}</p>}
      <div className="gdiv" />
      <div className="sidebar-foot">
        <button className="ghost" title="Settings (Ctrl+,)" onClick={() => navigate({ kind: "settings" })}><IconGear /> settings</button>
        <button className="ghost" title="Import a Granola CSV export" onClick={importGranola}>import</button>
        <button className="ghost boxed" title="Collapse sidebar (Ctrl+[)" onClick={() => setSidebarOpen(false)}><IconSidebar /></button>
      </div>
    </aside>
  );
}

// Minimal inline glyphs (SF Symbol equivalents): 14px, currentColor.
export function IconHome() {
  return <svg className="ico" viewBox="0 0 16 16"><path d="M2 8.5 8 3l6 5.5V14H10v-4H6v4H2z" fill="currentColor" /></svg>;
}
export function IconChart() {
  return <svg className="ico" viewBox="0 0 16 16"><rect x="2" y="8" width="3" height="6" fill="currentColor" /><rect x="6.5" y="4" width="3" height="10" fill="currentColor" /><rect x="11" y="6" width="3" height="8" fill="currentColor" /></svg>;
}
export function IconClock() {
  return <svg className="ico" viewBox="0 0 16 16"><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeWidth="1.6" /><path d="M8 4.5V8l2.5 1.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" /></svg>;
}
export function IconGear() {
  return <svg className="ico" viewBox="0 0 16 16"><circle cx="8" cy="8" r="2.2" fill="none" stroke="currentColor" strokeWidth="1.5" /><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" /></svg>;
}
export function IconSidebar() {
  return <svg className="ico" viewBox="0 0 16 16"><rect x="1.5" y="3" width="13" height="10" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.4" /><path d="M6 3v10" stroke="currentColor" strokeWidth="1.4" /></svg>;
}
