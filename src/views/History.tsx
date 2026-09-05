import { useEffect, useState } from "react";
import {
  deleteMeeting,
  getSegments,
  hasBridge,
  listMeetings,
  searchMeetings,
  setPinned,
} from "../api";
import type { Meeting, TranscriptSegment } from "../types";

function displayTitle(m: Meeting): string {
  const t = m.title.trim();
  return t.length > 0 ? t : "Untitled meeting";
}

function when(ts: number | null): string {
  if (!ts) return "";
  return new Date(ts * 1000).toLocaleString();
}

export function History() {
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Meeting | null>(null);
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);

  async function refresh() {
    if (!hasBridge) return;
    const list = query.trim() ? await searchMeetings(query) : await listMeetings();
    setMeetings(list);
  }

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query]);

  async function open(m: Meeting) {
    setSelected(m);
    setSegments(await getSegments(m.id));
  }

  async function togglePin(m: Meeting) {
    await setPinned(m.id, !m.pinned);
    await refresh();
  }

  async function remove(m: Meeting) {
    await deleteMeeting(m.id);
    if (selected?.id === m.id) {
      setSelected(null);
      setSegments([]);
    }
    await refresh();
  }

  return (
    <div className="view">
      <h1 className="view-title">History</h1>
      <input
        className="input"
        placeholder="Search meetings…"
        value={query}
        onChange={(e) => setQuery(e.currentTarget.value)}
      />

      <div className="history-grid">
        <div className="list">
          {meetings.length === 0 && <p className="muted">No meetings yet.</p>}
          {meetings.map((m) => (
            <div
              key={m.id}
              className={`list-item ${selected?.id === m.id ? "sel" : ""}`}
              onClick={() => open(m)}
            >
              <div className="li-main">
                <span className="li-title">
                  {m.pinned ? "★ " : ""}
                  {displayTitle(m)}
                </span>
                <span className="li-when">{when(m.started_at)}</span>
              </div>
              <div className="li-actions">
                <button className="mini" onClick={(e) => (e.stopPropagation(), togglePin(m))}>
                  {m.pinned ? "unpin" : "pin"}
                </button>
                <button className="mini danger" onClick={(e) => (e.stopPropagation(), remove(m))}>
                  delete
                </button>
              </div>
            </div>
          ))}
        </div>

        <div className="detail">
          {selected ? (
            <>
              <h2 className="card-title">{displayTitle(selected)}</h2>
              {selected.summary && <p className="muted">{selected.summary}</p>}
              <div className="lines">
                {segments.length === 0 && <p className="muted">No transcript segments.</p>}
                {segments.map((s) => (
                  <div className="line" key={s.id}>
                    <span className="spk">#{s.speaker}</span>
                    <span className="txt">{s.text}</span>
                  </div>
                ))}
              </div>
            </>
          ) : (
            <p className="muted">Select a meeting to view its transcript.</p>
          )}
        </div>
      </div>
    </div>
  );
}
