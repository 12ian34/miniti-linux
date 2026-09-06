import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  deleteMeeting,
  errorMessage,
  getLevels,
  getMeetingDetail,
  hasBridge,
  markAsYou,
  onTranscript,
  onTranscriptionStatus,
  setMeetingTitle,
  setNotes,
  setPinned,
  setSpeakerName,
} from "../api";
import { LevelMeter } from "../components/LevelMeter";
import {
  dateOnly,
  displayTitle,
  duration,
  elapsed,
  fallbackSpeakerLabel,
  speakerColor,
  streamStatusText,
  timeOnly,
} from "../format";
import { useStore } from "../store";
import { useTauriEvent } from "../useEvent";
import type { Levels, MeetingDetail, StreamStatus, TranscriptEventPayload } from "../types";
import { InsightsRail } from "./InsightsRail";
import { CatchUpButton, InvestigateButton } from "./MeetingTools";
import {
  deleteSegment,
  exportMarkdown,
  insightsFinishing,
  meetingMarkdown,
  onInsightsUpdated,
  onInvestigationSuggested,
  regenerateInsights,
  trimTranscript,
} from "../api";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

interface Line {
  key: string;
  speaker: number;
  text: string;
  source: string;
  final: boolean;
  start: number;
}

/** Interims replace in place per source; finals dedupe on their persisted id. */
function applyEvent(lines: Line[], ev: TranscriptEventPayload): Line[] {
  const interimKey = `interim:${ev.source}`;
  const without = lines.filter((l) => l.key !== interimKey);
  if (ev.is_final) {
    const key = ev.segment_id ? `seg:${ev.segment_id}` : `final:${ev.start}:${ev.speaker_id}`;
    if (without.some((l) => l.key === key)) return without;
    return [...without, { key, speaker: ev.speaker_id, text: ev.text, source: ev.source, final: true, start: ev.start }];
  }
  return [...without, { key: interimKey, speaker: ev.speaker_id, text: ev.text, source: ev.source, final: false, start: ev.start }];
}

export function MeetingView({ id }: { id: string }) {
  const store = useStore();
  const { recording, stop, stopping, navigate, refreshMeetings, insightsOpen, setInsightsOpen, prefs } = store;
  const live = recording.recording && recording.meeting_id === id;
  const [finishing, setFinishing] = useState(false);
  const [suggestedFocus, setSuggestedFocus] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [trimMode, setTrimMode] = useState(false);

  async function copyTranscript() {
    try {
      const md = await meetingMarkdown(id);
      const transcript = md.slice(md.indexOf("## Transcript"));
      await writeText(transcript);
      setNotice("Transcript copied.");
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  async function exportMd() {
    try {
      const path = await exportMarkdown(id);
      if (path) setNotice(`Exported to ${path}`);
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  async function removeTurn(keys: string[]) {
    try {
      for (const k of keys) {
        if (k.startsWith("seg:")) await deleteSegment(id, k.slice(4));
      }
      await load();
      setNotice("Removed. Regenerate insights from More to refresh them.");
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  async function trim(beforeS: number | null, afterS: number | null) {
    try {
      const n = await trimTranscript(id, beforeS, afterS);
      await load();
      setTrimMode(false);
      if (n > 0) {
        await regenerateInsights(id).catch(() => {});
        setNotice(`Trimmed ${n} segment${n === 1 ? "" : "s"}; regenerating insights.`);
      }
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [lines, setLines] = useState<Line[]>([]);
  const [status, setStatus] = useState<StreamStatus | null>(recording.stream);
  const [levels, setLevels] = useState<Levels>({ mic: 0, system: 0, recording: false });
  const [error, setError] = useState<string | null>(null);
  const [notes, setNotesState] = useState("");
  const [follow, setFollow] = useState(true);
  const [justStopped, setJustStopped] = useState(false);
  const notesTimer = useRef<number | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  const load = useCallback(async () => {
    if (!hasBridge) return;
    try {
      const d = await getMeetingDetail(id);
      if (!d) {
        setError("Meeting not found.");
        return;
      }
      setDetail(d);
      setNotesState(d.meeting.notes);
      setLines((prev) => {
        const interims = prev.filter((l) => !l.final);
        const finals: Line[] = d.segments.map((s) => ({
          key: `seg:${s.id}`,
          speaker: s.speaker,
          text: s.text,
          source: s.source,
          final: true,
          start: s.start_s,
        }));
        return [...finals, ...interims];
      });
    } catch (e) {
      setError(errorMessage(e));
    }
  }, [id]);

  useEffect(() => {
    void load();
  }, [load]);

  useTauriEvent(onTranscript, (ev) => {
    if (ev.meeting_id !== id) return;
    setLines((prev) => applyEvent(prev, ev));
  });
  useTauriEvent(onTranscriptionStatus, (st) => {
    if (live) setStatus(st);
  });
  useTauriEvent(onInsightsUpdated, (mid) => {
    if (mid === id) {
      void load();
      insightsFinishing().then((f) => setFinishing(f.includes(id))).catch(() => {});
    }
  });
  useTauriEvent(onInvestigationSuggested, (ev) => {
    if (ev.meeting_id === id && live) setSuggestedFocus(ev.focus);
  });
  useEffect(() => {
    if (!hasBridge) return;
    insightsFinishing().then((f) => setFinishing(f.includes(id))).catch(() => {});
  }, [id, live]);

  useEffect(() => {
    if (!live || !hasBridge) return;
    const t = window.setInterval(() => getLevels().then(setLevels).catch(() => {}), 150);
    return () => window.clearInterval(t);
  }, [live]);

  useEffect(() => {
    if (follow) bottomRef.current?.scrollIntoView({ block: "end" });
  }, [lines, follow]);

  function onScroll() {
    const el = scrollRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    setFollow(atBottom);
  }

  function onNotesChange(v: string) {
    setNotesState(v);
    if (notesTimer.current) window.clearTimeout(notesTimer.current);
    notesTimer.current = window.setTimeout(() => {
      setNotes(id, v).catch((e: unknown) => setError(errorMessage(e)));
    }, 400);
  }

  async function onStop() {
    try {
      await stop();
      setJustStopped(true);
      setLines((prev) => prev.filter((l) => l.final));
      await load();
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  async function rename() {
    if (!detail) return;
    const next = window.prompt("Meeting title", displayTitle(detail.meeting));
    if (next === null) return;
    await setMeetingTitle(id, next);
    await load();
    await refreshMeetings();
  }

  async function togglePin() {
    if (!detail) return;
    await setPinned(id, !detail.meeting.pinned);
    await load();
    await refreshMeetings();
  }

  async function remove() {
    if (!detail || live) return;
    if (!window.confirm(`Delete “${displayTitle(detail.meeting)}”? This cannot be undone.`)) return;
    await deleteMeeting(id);
    await refreshMeetings();
    navigate({ kind: "home" });
  }

  const label = useCallback(
    (sid: number) => detail?.speaker_labels[String(sid)] ?? fallbackSpeakerLabel(sid),
    [detail],
  );
  const selfIds = useMemo<number[]>(() => {
    try {
      const ids = JSON.parse(detail?.meeting.self_speaker_ids || "[]");
      return Array.isArray(ids) && ids.length ? ids : [1000];
    } catch {
      return [1000];
    }
  }, [detail]);
  const isYou = (sid: number) => selfIds.includes(sid);

  async function renameSpeaker(sid: number) {
    const next = window.prompt("Speaker name (empty to clear)", label(sid) === "You" ? "" : label(sid));
    if (next === null) return;
    await setSpeakerName(id, sid, next);
    await load();
  }
  async function toggleYou(sid: number) {
    await markAsYou(id, sid, !isYou(sid));
    await load();
  }

  const speakerIds = useMemo(
    () => Array.from(new Set(lines.filter((l) => l.final).map((l) => l.speaker))).sort((a, b) => a - b),
    [lines],
  );

  const m = detail?.meeting;

  return (
    <div className={`meeting ${insightsOpen ? "" : "insights-collapsed"}`}>
      <div className="meeting-main">
        <header className={`terminal-header ${live ? "live" : ""}`}>
          <div className="th-left">
            {live ? (
              <>
                <span className="dot dot-rec pulse" />
                <span className="timer">{elapsed(recording.elapsed_seconds)}</span>
                <span className="muted small">{streamStatusText(status, true)}</span>
              </>
            ) : (
              <>
                <button className="ghost" onClick={() => navigate({ kind: "home" })} title="Back to meetings">
                  ‹ meetings
                </button>
                {m && (
                  <span className="muted small">
                    {dateOnly(m.started_at)} · {timeOnly(m.started_at)}
                    {duration(m) ? ` · ${duration(m)}` : ""}
                    {m.import_source ? ` · imported from ${m.import_source}` : ""}
                  </span>
                )}
              </>
            )}
          </div>
          <div className="th-right">
            {detail && (lines.some((l) => l.final)) && (
              <>
                <CatchUpButton meetingId={id} />
                <InvestigateButton
                  meetingId={id}
                  suggestedFocus={suggestedFocus}
                  onDismissSuggestion={() => setSuggestedFocus(null)}
                  hasCodebaseRoot={!!prefs?.codebase_root}
                />
              </>
            )}
            {live ? (
              <button className="btn recording" onClick={onStop} disabled={stopping}>
                {stopping ? "finishing…" : "■ Stop"}
              </button>
            ) : (
              <>
                <button className="ghost" onClick={copyTranscript} title="Copy transcript">copy</button>
                <button className="ghost" onClick={exportMd} title="Export as Markdown">export</button>
                <button className={`ghost ${trimMode ? "on" : ""}`} onClick={() => setTrimMode(!trimMode)} title="Trim transcript">
                  trim
                </button>
                <button className="ghost" onClick={togglePin} title="Pin">
                  {m?.pinned ? "★" : "☆"}
                </button>
                <button className="ghost" onClick={remove} title="Delete">
                  delete
                </button>
              </>
            )}
            <button className="ghost" title="Toggle insights (Ctrl+])" onClick={() => setInsightsOpen(!insightsOpen)}>
              {insightsOpen ? "›" : "‹"}
            </button>
          </div>
        </header>

        <div className="meeting-title-row">
          <h1 className="meeting-title" onClick={rename} title="Rename">
            {m ? displayTitle(m) : "…"}
          </h1>
        </div>

        {error && <div className="banner error">{error}</div>}
        {notice && (
          <div className="banner ok">
            {notice}
            <button className="ghost" onClick={() => setNotice(null)}>×</button>
          </div>
        )}
        {trimMode && !live && (
          <div className="banner warn">
            Trim mode: hover a turn to remove it, or cut everything before / after it. Insights regenerate afterwards.
            <button className="ghost" onClick={() => setTrimMode(false)}>done</button>
          </div>
        )}
        {justStopped && !live && (
          <div className="banner ok">
            Saved.{finishing ? " Final insights are being generated in the background." : ""}
            <button className="ghost" onClick={() => navigate({ kind: "home" })}>back to meetings</button>
          </div>
        )}

        {live && (
          <div className="meters">
            <LevelMeter label="mic" level={levels.mic} active tone="mic" />
            <LevelMeter label="system" level={levels.system} active tone="system" />
          </div>
        )}

        {speakerIds.length > 0 && (
          <div className="speakers">
            {speakerIds.map((sid) => (
              <div className="speaker-chip" key={sid} style={{ borderColor: speakerColor(sid, isYou(sid)) }}>
                <span style={{ color: speakerColor(sid, isYou(sid)) }}>{label(sid)}</span>
                <button className="ghost tiny" onClick={() => renameSpeaker(sid)}>rename</button>
                <button className="ghost tiny" onClick={() => toggleYou(sid)}>
                  {isYou(sid) ? "not me" : "mark as you"}
                </button>
              </div>
            ))}
          </div>
        )}

        <div className="split">
          <section className="transcript" ref={scrollRef} onScroll={onScroll}>
            {lines.length === 0 ? (
              <p className="muted pad">{live ? "Listening…" : "No transcript."}</p>
            ) : (
              <TranscriptBody
                lines={lines}
                label={label}
                isYou={isYou}
                trim={trimMode && !live}
                onRemove={removeTurn}
                onTrimBefore={(t) => trim(t, null)}
                onTrimAfter={(t) => trim(null, t)}
              />
            )}
            <div ref={bottomRef} />
            {!follow && live && (
              <button className="btn secondary resume" onClick={() => setFollow(true)}>
                ↓ resume
              </button>
            )}
          </section>
          <section className="notes">
            <div className="panel-title">notes</div>
            <textarea
              className="notes-area"
              placeholder="Private notes… (saved automatically)"
              value={notes}
              onChange={(e) => onNotesChange(e.currentTarget.value)}
            />
          </section>
        </div>
      </div>

      {insightsOpen && detail && (
        <InsightsRail meetingId={id} meeting={detail.meeting} live={live} finishing={finishing} onMeetingChanged={load} />
      )}
    </div>
  );
}

function TranscriptBody({
  lines,
  label,
  isYou,
  trim,
  onRemove,
  onTrimBefore,
  onTrimAfter,
}: {
  lines: Line[];
  label: (id: number) => string;
  isYou: (id: number) => boolean;
  trim?: boolean;
  onRemove?: (keys: string[]) => void;
  onTrimBefore?: (startS: number) => void;
  onTrimAfter?: (startS: number) => void;
}) {
  // Group consecutive lines by speaker, like the macOS speaker-turn document.
  const turns: { speaker: number; lines: Line[] }[] = [];
  for (const l of lines) {
    const last = turns[turns.length - 1];
    if (last && last.speaker === l.speaker) last.lines.push(l);
    else turns.push({ speaker: l.speaker, lines: [l] });
  }
  return (
    <div className="turns">
      {turns.map((t, i) => (
        <div className={`turn ${isYou(t.speaker) ? "you" : ""}`} key={`${t.speaker}-${i}`}>
          <div className="turn-speaker" style={{ color: speakerColor(t.speaker, isYou(t.speaker)) }}>
            {label(t.speaker)}
            {trim && (
              <span className="turn-tools">
                <button className="ghost tiny" onClick={() => onTrimBefore?.(t.lines[0].start)} title="Remove everything before this turn">⇤ cut before</button>
                <button className="ghost tiny" onClick={() => onRemove?.(t.lines.map((l) => l.key))} title="Remove this turn">✕</button>
                <button className="ghost tiny" onClick={() => onTrimAfter?.(t.lines[t.lines.length - 1].start)} title="Remove everything after this turn">cut after ⇥</button>
              </span>
            )}
          </div>
          <div className="turn-text">
            {t.lines.map((l) => (
              <span key={l.key} className={l.final ? "" : "interim"}>
                {l.text}{" "}
              </span>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
