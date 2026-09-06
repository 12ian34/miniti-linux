import { useEffect, useMemo, useState } from "react";
import { hasBridge, insightsFinishing, onInsightsStatus, setPinned } from "../api";
import { useTauriEvent } from "../useEvent";
import { displayTitle, duration, groupMeetings, timeOnly } from "../format";
import { useStore } from "../store";
import type { Meeting } from "../types";

export function Sidebar() {
  const store = useStore();
  const { route, navigate, meetings, sidebarOpen, setSidebarOpen, recording, refreshMeetings } = store;
  const [query, setQuery] = useState("");
  const [finishing, setFinishing] = useState<string[]>([]);
  useEffect(() => {
    if (!hasBridge) return;
    insightsFinishing().then(setFinishing).catch(() => {});
  }, [meetings]);
  useTauriEvent(onInsightsStatus, () => {
    insightsFinishing().then(setFinishing).catch(() => {});
  });

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return meetings;
    return meetings.filter(
      (m) =>
        displayTitle(m).toLowerCase().includes(q) ||
        m.summary.toLowerCase().includes(q) ||
        m.notes.toLowerCase().includes(q) ||
        m.topics.toLowerCase().includes(q),
    );
  }, [meetings, query]);

  const groups = useMemo(() => groupMeetings(filtered), [filtered]);
  const selectedId = route.kind === "meeting" ? route.id : null;

  async function togglePin(e: React.MouseEvent, m: Meeting) {
    e.stopPropagation();
    await setPinned(m.id, !m.pinned);
    await refreshMeetings();
  }

  if (!sidebarOpen) {
    return (
      <aside className="rail">
        <button className="rail-btn brand-mark" title="Home" onClick={() => navigate({ kind: "home" })}>
          m
        </button>
        <button className="rail-btn" title="Show history (Ctrl+[)" onClick={() => setSidebarOpen(true)}>
          ☰
        </button>
        <button
          className={`rail-btn ${route.kind === "coaching" ? "active" : ""}`}
          title="Coaching"
          onClick={() => navigate({ kind: "coaching" })}
        >
          ◔
        </button>
        {recording.recording && <span className="rail-rec" title="Recording" />}
        <div className="rail-spacer" />
        <button className="rail-btn" title="Settings (Ctrl+,)" onClick={() => navigate({ kind: "settings" })}>
          ⚙
        </button>
      </aside>
    );
  }

  return (
    <aside className="sidebar">
      <div className="sidebar-head">
        <button className="brand" onClick={() => navigate({ kind: "home" })}>
          <span className="brand-mark">miniti</span>
        </button>
        <button className="ghost" title="Collapse (Ctrl+[)" onClick={() => setSidebarOpen(false)}>
          ‹
        </button>
      </div>

      <nav className="side-nav">
        <button
          className={`side-item ${route.kind === "home" ? "active" : ""}`}
          onClick={() => navigate({ kind: "home" })}
        >
          home
        </button>
        <button
          className={`side-item ${route.kind === "coaching" ? "active" : ""}`}
          onClick={() => navigate({ kind: "coaching" })}
        >
          coaching
        </button>
      </nav>

      <input
        className="input search"
        placeholder="search meetings"
        value={query}
        onChange={(e) => setQuery(e.currentTarget.value)}
      />

      <div className="side-list">
        {groups.length === 0 && <p className="muted pad">No meetings yet.</p>}
        {groups.map((g) => (
          <div key={g.label} className="side-group">
            <div className="side-group-label">{g.label}</div>
            {g.meetings.map((m) => {
              const live = recording.recording && recording.meeting_id === m.id;
              return (
                <div
                  key={m.id}
                  className={`side-row ${selectedId === m.id ? "active" : ""}`}
                  onClick={() => navigate({ kind: "meeting", id: m.id })}
                >
                  <div className="side-row-title">
                    {live && <span className="dot dot-rec" />}
                    <span className="ellipsis">{displayTitle(m)}</span>
                  </div>
                  <div className="side-row-meta">
                    {finishing.includes(m.id) ? (
                      <span className="finishing">finishing insights…</span>
                    ) : (
                      <>
                        <span>{timeOnly(m.started_at)}</span>
                        {duration(m) && <span>{duration(m)}</span>}
                      </>
                    )}
                    {m.import_source && <span className="pill">{m.import_source}</span>}
                    <button className="ghost pin" title={m.pinned ? "Unpin" : "Pin"} onClick={(e) => togglePin(e, m)}>
                      {m.pinned ? "★" : "☆"}
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        ))}
      </div>

      <div className="sidebar-foot">
        <button className="ghost" onClick={() => navigate({ kind: "settings" })}>
          ⚙ settings
        </button>
        <span className="muted tiny">multi-dimensional meetings</span>
      </div>
    </aside>
  );
}
