import { useCallback, useEffect, useState } from "react";
import {
  deleteMeeting,
  errorMessage,
  getMeetingDetail,
  hasBridge,
  listMeetings,
  markAsYou,
  onMeetingSaved,
  searchMeetings,
  setMeetingTitle,
  setPinned,
  setSpeakerName,
} from "../api";
import { displayTitle, duration, fallbackSpeakerLabel, when } from "../format";
import type { Meeting, MeetingDetail } from "../types";

export function History() {
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [query, setQuery] = useState("");
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!hasBridge) return;
    try {
      const list = query.trim() ? await searchMeetings(query) : await listMeetings();
      setMeetings(list);
    } catch (e) {
      setError(errorMessage(e));
    }
  }, [query]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!hasBridge) return;
    let unlisten: (() => void) | undefined;
    onMeetingSaved(() => void refresh()).then((fn) => (unlisten = fn));
    return () => unlisten?.();
  }, [refresh]);

  async function open(id: string) {
    try {
      setDetail(await getMeetingDetail(id));
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  async function reopen() {
    if (detail) await open(detail.meeting.id);
    await refresh();
  }

  async function togglePin(m: Meeting) {
    await setPinned(m.id, !m.pinned);
    await reopen();
  }

  async function remove(m: Meeting) {
    if (!window.confirm(`Delete “${displayTitle(m)}”? This cannot be undone.`)) return;
    await deleteMeeting(m.id);
    if (detail?.meeting.id === m.id) setDetail(null);
    await refresh();
  }

  async function rename() {
    if (!detail) return;
    const next = window.prompt("Meeting title", displayTitle(detail.meeting));
    if (next === null) return;
    await setMeetingTitle(detail.meeting.id, next);
    await reopen();
  }

  async function renameSpeaker(speakerId: number, current: string) {
    if (!detail) return;
    const next = window.prompt("Speaker name (empty to clear)", current);
    if (next === null) return;
    await setSpeakerName(detail.meeting.id, speakerId, next);
    await reopen();
  }

  async function toggleYou(speakerId: number) {
    if (!detail) return;
    const selfIds: number[] = JSON.parse(detail.meeting.self_speaker_ids || "[]");
    const isYou = selfIds.length > 0 ? selfIds.includes(speakerId) : speakerId === 1000;
    await markAsYou(detail.meeting.id, speakerId, !isYou);
    await reopen();
  }

  const label = (id: number) =>
    detail?.speaker_labels[String(id)] ?? fallbackSpeakerLabel(id);

  const speakerIds = detail
    ? Array.from(new Set(detail.segments.map((s) => s.speaker))).sort((a, b) => a - b)
    : [];

  return (
    <div className="view">
      <h1 className="view-title">History</h1>
      <input
        className="input"
        placeholder="Search meetings…"
        value={query}
        onChange={(e) => setQuery(e.currentTarget.value)}
      />
      {error && <div className="banner error">{error}</div>}

      <div className="history-grid">
        <div className="list">
          {meetings.length === 0 && <p className="muted">No meetings yet.</p>}
          {meetings.map((m) => (
            <div
              key={m.id}
              className={`list-item ${detail?.meeting.id === m.id ? "sel" : ""}`}
              onClick={() => open(m.id)}
            >
              <div className="li-main">
                <span className="li-title">
                  {m.pinned ? "★ " : ""}
                  {displayTitle(m)}
                </span>
                <span className="li-when">
                  {when(m.started_at)}
                  {duration(m) ? ` · ${duration(m)}` : ""}
                </span>
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
          {detail ? (
            <>
              <div className="detail-head">
                <h2 className="card-title">{displayTitle(detail.meeting)}</h2>
                <button className="mini" onClick={rename}>
                  rename
                </button>
              </div>
              <p className="muted">
                {when(detail.meeting.started_at)}
                {duration(detail.meeting) ? ` · ${duration(detail.meeting)}` : ""}
                {detail.meeting.language ? ` · ${detail.meeting.language}` : ""}
              </p>
              {detail.meeting.summary && <p className="muted">{detail.meeting.summary}</p>}

              {speakerIds.length > 0 && (
                <div className="speakers">
                  {speakerIds.map((id) => (
                    <div className="speaker-chip" key={id}>
                      <span className="spk">{label(id)}</span>
                      <button className="mini" onClick={() => renameSpeaker(id, label(id))}>
                        rename
                      </button>
                      <button className="mini" onClick={() => toggleYou(id)}>
                        {label(id) === "You" ? "not me" : "mark as you"}
                      </button>
                    </div>
                  ))}
                </div>
              )}

              <div className="lines">
                {detail.segments.length === 0 && (
                  <p className="muted">No transcript segments.</p>
                )}
                {detail.segments.map((s) => (
                  <div className="line" key={s.id}>
                    <span className="spk">{label(s.speaker)}</span>
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
