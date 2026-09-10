import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  deleteMeeting,
  deleteSegment,
  errorMessage,
  exportMarkdown,
  getLevels,
  getMeetingDetail,
  hasBridge,
  insightsFinishing,
  markAsYou,
  meetingMarkdown,
  onInsightsUpdated,
  onInvestigationSuggested,
  onTranscript,
  onTranscriptionStatus,
  regenerateInsights,
  setMeetingTitle,
  setNotes,
  setPinned,
  setSpeakerName,
  trimTranscript,
  addCorrection,
  onTranscriptCorrected,
} from "../api";
import { LevelMeter } from "../components/LevelMeter";
import { dateOnly, displayTitle, duration, elapsed, fallbackSpeakerLabel, speakerColor, streamStatusText, timeOnly } from "../format";
import { useStore } from "../store";
import { useTauriEvent } from "../useEvent";
import type { Levels, MeetingDetail, StreamStatus, TranscriptEventPayload } from "../types";
import { InsightsRail } from "./InsightsRail";
import { CatchUpButton, InvestigateButton, Sheet } from "./MeetingTools";
import { CrmSheet } from "./CrmSheet";

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

function ts(s: number): string {
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return `${m}:${sec.toString().padStart(2, "0")}`;
}

/** Port of the macOS MeetingView: TerminalHeader · speaker legend · transcript + notes · insights. */
export function MeetingView({ id }: { id: string }) {
  const store = useStore();
  const { recording, stop, stopping, navigate, refreshMeetings, insightsOpen, setInsightsOpen, prefs } = store;
  const live = recording.recording && recording.meeting_id === id;

  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [lines, setLines] = useState<Line[]>([]);
  const [status, setStatus] = useState<StreamStatus | null>(recording.stream);
  const [levels, setLevels] = useState<Levels>({ mic: 0, system: 0, recording: false });
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [notes, setNotesState] = useState("");
  const [follow, setFollow] = useState(true);
  const [justStopped, setJustStopped] = useState(false);
  const [finishing, setFinishing] = useState(false);
  const [suggestedFocus, setSuggestedFocus] = useState<string | null>(null);
  const [trimMode, setTrimMode] = useState(false);
  const [crmOpen, setCrmOpen] = useState(false);
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  const [speakerMenu, setSpeakerMenu] = useState<number | null>(null);
  const [renaming, setRenaming] = useState<{ sid: number; name: string; you: boolean } | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  // Dictionary corrections: a selection inside a final turn offers "correct";
  // the editor is a sheet so the selection can vanish without breaking it.
  const [selection, setSelection] = useState<{ heard: string; x: number; y: number } | null>(null);
  const [correcting, setCorrecting] = useState<{ heard: string; correct: string; fixEarlier: boolean } | null>(null);
  const notesTimer = useRef<number | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  // ResizableNotesLayout: notes default 100, min 50, max 500; transcript keeps ≥ 200.
  const [notesHeight, setNotesHeight] = useState(() => {
    try { return Number(localStorage.getItem("ui.notesHeight")) || 100; } catch { return 100; }
  });
  const splitRef = useRef<HTMLDivElement>(null);
  const dragStart = useRef<{ y: number; h: number } | null>(null);
  function onHandleDown(e: React.MouseEvent) {
    dragStart.current = { y: e.clientY, h: notesHeight };
    const move = (ev: MouseEvent) => {
      if (!dragStart.current) return;
      const avail = splitRef.current?.clientHeight ?? 600;
      const max = Math.min(500, Math.max(50, avail - 200 - 8));
      const h = Math.min(max, Math.max(50, dragStart.current.h + (dragStart.current.y - ev.clientY)));
      setNotesHeight(h);
    };
    const up = () => {
      dragStart.current = null;
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      try { localStorage.setItem("ui.notesHeight", String(notesHeight)); } catch { /* ignore */ }
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  const load = useCallback(async () => {
    if (!hasBridge) return;
    try {
      const d = await getMeetingDetail(id);
      if (!d) { setError("Meeting not found."); return; }
      setDetail(d);
      setNotesState(d.meeting.notes);
      setLines((prev) => {
        const interims = prev.filter((l) => !l.final);
        const finals: Line[] = d.segments.map((s) => ({ key: `seg:${s.id}`, speaker: s.speaker, text: s.text, source: s.source, final: true, start: s.start_s }));
        return [...finals, ...interims];
      });
    } catch (e) {
      setError(errorMessage(e));
    }
  }, [id]);
  useEffect(() => { void load(); }, [load]);

  useTauriEvent(onTranscript, (ev) => { if (ev.meeting_id === id) setLines((prev) => applyEvent(prev, ev)); });
  useTauriEvent(onTranscriptCorrected, (mid) => { if (mid === id) void load(); });

  /** Snap the current selection to whole words inside one final turn, or clear. */
  function readSelection() {
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || sel.rangeCount === 0) { setSelection(null); return; }
    const range = sel.getRangeAt(0);
    const node = range.commonAncestorContainer;
    const el = node instanceof Element ? node : node.parentElement;
    const turnText = el?.closest(".turn-text");
    if (!turnText || turnText.closest(".turn.interim")) { setSelection(null); return; }
    const heard = sel.toString().replace(/\s+/g, " ").trim().replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}#+]+$/gu, "");
    if (!heard || heard.length > 80) { setSelection(null); return; }
    const rect = range.getBoundingClientRect();
    setSelection({ heard, x: rect.left + rect.width / 2, y: rect.top });
  }
  function openCorrection(heard: string) {
    setSelection(null);
    setCorrecting({ heard, correct: "", fixEarlier: true });
  }
  async function saveCorrection() {
    if (!correcting || !correcting.correct.trim()) return;
    try {
      await addCorrection(correcting.heard, correcting.correct, id, correcting.fixEarlier);
      setNotice(`"${correcting.heard}" is now "${correcting.correct.trim()}" (also for the next meeting)`);
      setCorrecting(null);
      window.getSelection()?.removeAllRanges();
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && (e.key === "D" || e.key === "d")) {
        const sel = window.getSelection()?.toString().trim();
        if (sel && sel.length <= 80) { e.preventDefault(); openCorrection(sel.replace(/\s+/g, " ")); }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  useTauriEvent(onTranscriptionStatus, (st) => { if (live) setStatus(st); });
  useTauriEvent(onInsightsUpdated, (mid) => {
    if (mid === id) {
      void load();
      insightsFinishing().then((f) => setFinishing(f.includes(id))).catch(() => {});
    }
  });
  useTauriEvent(onInvestigationSuggested, (ev) => { if (ev.meeting_id === id && live) setSuggestedFocus(ev.focus); });
  useEffect(() => {
    if (!hasBridge) return;
    insightsFinishing().then((f) => setFinishing(f.includes(id))).catch(() => {});
  }, [id, live]);
  useEffect(() => {
    if (!live || !hasBridge) return;
    const t = window.setInterval(() => getLevels().then(setLevels).catch(() => {}), 150);
    return () => window.clearInterval(t);
  }, [live]);
  useEffect(() => { if (follow) bottomRef.current?.scrollIntoView({ block: "end" }); }, [lines, follow]);

  function onScroll() {
    const el = scrollRef.current;
    if (!el) return;
    setFollow(el.scrollHeight - el.scrollTop - el.clientHeight < 40);
  }
  function onNotesChange(v: string) {
    setNotesState(v);
    if (notesTimer.current) window.clearTimeout(notesTimer.current);
    notesTimer.current = window.setTimeout(() => { setNotes(id, v).catch((e: unknown) => setError(errorMessage(e))); }, 400);
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
  async function commitTitle() {
    setEditingTitle(false);
    if (!detail || titleDraft.trim() === displayTitle(detail.meeting)) return;
    await setMeetingTitle(id, titleDraft);
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
  async function copySection(section: "transcript" | "notes" | "insights" | "all") {
    try {
      const md = await meetingMarkdown(id);
      const cut = (start: string, ends: string[]) => {
        const i = md.indexOf(start);
        if (i < 0) return "";
        const rest = md.slice(i);
        const j = ends.map((e) => rest.indexOf(e, start.length)).filter((x) => x > 0).sort((a, b) => a - b)[0];
        return j ? rest.slice(0, j) : rest;
      };
      const text = section === "all" ? md
        : section === "transcript" ? cut("## Transcript", [])
        : section === "notes" ? (notes.trim() ? `## Notes\n\n${notes}` : "")
        : cut("## Insights", ["\n---\n"]);
      await writeText(text.trim());
      setCopied(section);
      window.setTimeout(() => setCopied(null), 1500);
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  async function exportMd() {
    try {
      const path = await exportMarkdown(id);
      if (path) setNotice(`exported to ${path}`);
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  async function removeTurn(keys: string[]) {
    try {
      for (const k of keys) if (k.startsWith("seg:")) await deleteSegment(id, k.slice(4));
      await load();
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
        setNotice(`trimmed ${n} segment${n === 1 ? "" : "s"} · regenerating insights`);
      }
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  const label = useCallback((sid: number) => detail?.speaker_labels[String(sid)] ?? fallbackSpeakerLabel(sid), [detail]);
  const selfIds = useMemo<number[]>(() => {
    try {
      const ids = JSON.parse(detail?.meeting.self_speaker_ids || "[]");
      return Array.isArray(ids) && ids.length ? ids : [1000];
    } catch { return [1000]; }
  }, [detail]);
  const isYou = (sid: number) => selfIds.includes(sid);
  function renameSpeaker(sid: number) {
    setSpeakerMenu(null);
    setRenaming({ sid, name: isYou(sid) ? "" : label(sid), you: isYou(sid) });
  }
  async function saveRename() {
    if (!renaming) return;
    const { sid, name, you } = renaming;
    setRenaming(null);
    try {
      if (you !== isYou(sid)) await markAsYou(id, sid, you);
      if (!you) await setSpeakerName(id, sid, name.trim());
      await load();
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  async function toggleYou(sid: number) {
    setSpeakerMenu(null);
    await markAsYou(id, sid, !isYou(sid));
    await load();
  }

  const speakerIds = useMemo(() => Array.from(new Set(lines.filter((l) => l.final).map((l) => l.speaker))).sort((a, b) => a - b), [lines]);
  const selfCount = speakerIds.filter(isYou).length;
  const remoteCount = speakerIds.length - selfCount;
  const attendees: { self?: boolean; is_self?: boolean }[] = useMemo(() => {
    try { return JSON.parse(detail?.meeting.attendees || "[]"); } catch { return []; }
  }, [detail]);
  const m = detail?.meeting;

  return (
    <div className={`meeting ${insightsOpen ? "" : "insights-collapsed"}`}>
      <div className="meeting-main">
        <header className="terminal-header">
          <div className="th-title-row">
            {editingTitle ? (
              <input
                className="title-input"
                autoFocus
                value={titleDraft}
                onChange={(e) => setTitleDraft(e.currentTarget.value)}
                onBlur={commitTitle}
                onKeyDown={(e) => { if (e.key === "Enter") commitTitle(); if (e.key === "Escape") setEditingTitle(false); }}
              />
            ) : (
              <button className="title-btn" title="rename" onClick={() => { setTitleDraft(m ? displayTitle(m) : ""); setEditingTitle(true); }}>
                {m ? displayTitle(m) : "…"}
              </button>
            )}
            {m && !live && (
              <span className="muted small">
                {dateOnly(m.started_at)} · {timeOnly(m.started_at)}{duration(m) ? ` · ${duration(m)}` : ""}
                {m.import_source ? ` · imported from ${m.import_source.split(":")[0]}` : ""}
              </span>
            )}
            {live && (
              <span className="th-live">
                <span className="dot dot-rec pulse" />
                <span className="timer">{elapsed(recording.elapsed_seconds)}</span>
                <LevelMeter label="mic" level={levels.mic} active tone="mic" />
                {prefs?.capture_system_audio && <LevelMeter label="system" level={levels.system} active tone="system" />}
                <span className={`muted tiny ${status?.state === "failed" || status?.state === "reconnecting" ? "warn-text" : ""}`}>{streamStatusText(status, true)}</span>
              </span>
            )}
          </div>

          <div className="th-actions">
            {live ? (
              <button className="control recording emph" onClick={onStop} disabled={stopping}>
                <span className="glyph">■</span> {stopping ? "finishing…" : "stop"} <span className="kbd light">Ctrl+R</span>
              </button>
            ) : (
              <button className="control primary emph" onClick={() => navigate({ kind: "home" })}>
                <span className="glyph">▤</span> back to meetings
              </button>
            )}
            {detail && lines.some((l) => l.final) && (
              <>
                <InvestigateButton meetingId={id} suggestedFocus={suggestedFocus} onDismissSuggestion={() => setSuggestedFocus(null)} hasCodebaseRoot={!!prefs?.codebase_root} />
                <CatchUpButton meetingId={id} />
              </>
            )}
            {attendees.length > 0 && (
              <span className="control quiet" title="attendees"><span className="glyph">👥</span> {attendees.filter((a) => !(a.self || a.is_self)).length}</span>
            )}
            {!live && m && (
              <>
                <button className="control quiet" onClick={() => setCrmOpen(true)} title="send to Attio / Twenty"><span className="glyph">↗</span> crm</button>
                <button className="control quiet" onClick={exportMd} title="export Markdown"><span className="glyph">⇩</span> export</button>
                <button className={`control quiet ${trimMode ? "on" : ""}`} onClick={() => setTrimMode(!trimMode)} title="trim transcript"><span className="glyph">✂</span> trim</button>
                <button className="control quiet" onClick={togglePin} title={m.pinned ? "unpin" : "pin"}><span className="glyph">{m.pinned ? "★" : "☆"}</span> {m.pinned ? "pinned" : "pin"}</button>
                <button className="control destructive" onClick={remove} title="delete meeting"><span className="glyph">🗑</span> delete</button>
              </>
            )}
            <span className="th-spacer" />
            <button className="ghost boxed" title="Toggle insights (Ctrl+])" onClick={() => setInsightsOpen(!insightsOpen)}>{insightsOpen ? "▸" : "◂"}</button>
          </div>
        </header>
        <div className="gdiv" />

        {error && <div className="banner error inset">{error}<button className="ghost" onClick={() => setError(null)}>×</button></div>}
        {notice && <div className="banner ok inset">{notice}<button className="ghost" onClick={() => setNotice(null)}>×</button></div>}
        {justStopped && !live && (
          <div className="banner ok inset">
            saved{finishing ? " · final insights are being generated in the background" : ""}
            <button className="ghost" onClick={() => setJustStopped(false)}>×</button>
          </div>
        )}
        {trimMode && !live && (
          <div className="banner warn inset">
            trim: hover a turn to remove it or cut everything before / after it. insights regenerate afterwards.
            <button className="ghost" onClick={() => setTrimMode(false)}>done</button>
          </div>
        )}

        {/* Speaker legend (macOS SpeakerLegend): tiny dots + 10px labels, click to rename / mark as you. */}
        <div className="legend" onMouseLeave={() => setSpeakerMenu(null)}>
          {live && <span className="legend-live"><span className="dot dot-rec" /> live</span>}
          <span className="legend-k">speakers:</span>
          {speakerIds.length === 0 && <span className="legend-k">detecting…</span>}
          {speakerIds.map((sid) => (
            <span className="legend-chip-wrap" key={sid}>
              <button className="legend-chip" style={{ color: speakerColor(sid, isYou(sid)) }} onClick={() => setSpeakerMenu(speakerMenu === sid ? null : sid)} title="rename speaker">
                <span className="dot" style={{ background: speakerColor(sid, isYou(sid)) }} />{label(sid)}
              </button>
              {speakerMenu === sid && (
                <span className="menu legend-menu">
                  <button onClick={() => renameSpeaker(sid)}>rename…</button>
                  <button onClick={() => toggleYou(sid)}>{isYou(sid) ? "not me" : "mark as you"}</button>
                </span>
              )}
            </span>
          ))}
          {speakerIds.length === 1 && <span className="legend-k">{isYou(speakerIds[0]) ? "(you only)" : "(single speaker)"}</span>}
          {selfCount >= 1 && remoteCount === 1 && <span className="legend-k">{selfCount > 1 ? `(you ×${selfCount} + 1 remote)` : "(you + 1 remote)"}</span>}
        </div>

        <div className="split" ref={splitRef}>
          <section className="transcript-pane">
            <SectionHeader icon="¶" title="transcript" copied={copied === "transcript"} onCopy={() => copySection("transcript")} />
            <div className="transcript" ref={scrollRef} onScroll={() => { onScroll(); setSelection(null); }} onMouseUp={readSelection} onKeyUp={readSelection}>
              {lines.length === 0 ? (
                <p className="muted pad">{live ? "listening…" : "no transcript."}</p>
              ) : (
                <TranscriptBody lines={lines} label={label} isYou={isYou} trim={trimMode && !live} onRemove={removeTurn} onTrimBefore={(t) => trim(t, null)} onTrimAfter={(t) => trim(null, t)} />
              )}
              <div ref={bottomRef} />
            </div>
            {!follow && live && <button className="control resume" onClick={() => { setFollow(true); bottomRef.current?.scrollIntoView({ block: "end" }); }}>↓ resume</button>}
          </section>
          <div className="drag-handle" onMouseDown={onHandleDown} title="drag to resize notes"><span /></div>
          <section className="notes-pane" style={{ height: notesHeight }}>
            <SectionHeader icon="✎" title="notes" copied={copied === "notes"} onCopy={() => copySection("notes")} />
            <textarea className="notes-area" placeholder="relax and take notes..." value={notes} onChange={(e) => onNotesChange(e.currentTarget.value)} />
          </section>
        </div>
      </div>

      {selection && (
        <button className="correct-pill" style={{ left: selection.x, top: selection.y }} onMouseDown={(e) => e.preventDefault()} onClick={() => openCorrection(selection.heard)} title="Ctrl+Shift+D">
          correct “{selection.heard.length > 24 ? selection.heard.slice(0, 24) + "…" : selection.heard}”
        </button>
      )}
      {correcting && (
        <Sheet title="correct a word" onClose={() => setCorrecting(null)}>
          <div className="speaker-sheet">
            <p className="muted small">the corrected spelling is applied to this transcript as it arrives, remembered for future meetings, and sent to deepgram on the next connect. insights are not affected.</p>
            <label className="field"><span className="muted tiny">heard</span>
              <input className="input" value={correcting.heard} onChange={(e) => setCorrecting({ ...correcting, heard: e.currentTarget.value })} />
            </label>
            <label className="field"><span className="muted tiny">correct to</span>
              <input className="input" autoFocus value={correcting.correct} onChange={(e) => setCorrecting({ ...correcting, correct: e.currentTarget.value })}
                onKeyDown={(e) => { if (e.key === "Enter") void saveCorrection(); if (e.key === "Escape") setCorrecting(null); }} />
            </label>
            <label className="toggle">
              <input type="checkbox" checked={correcting.fixEarlier} onChange={(e) => setCorrecting({ ...correcting, fixEarlier: e.currentTarget.checked })} />
              <span>fix earlier mentions in this meeting</span>
            </label>
            <div className="save-row">
              <button className="control primary" disabled={!correcting.correct.trim()} onClick={saveCorrection}>save</button>
              <button className="control" onClick={() => setCorrecting(null)}>cancel</button>
            </div>
          </div>
        </Sheet>
      )}
      {crmOpen && <CrmSheet meetingId={id} onClose={() => setCrmOpen(false)} />}
      {renaming && (
        <Sheet title="speaker" onClose={() => setRenaming(null)}>
          <div className="speaker-sheet">
            <p className="muted small"><span className="swatch" style={{ background: speakerColor(renaming.sid, renaming.you) }} />{fallbackSpeakerLabel(renaming.sid)} · rename for this meeting, or mark the voice as yours so coaching counts it</p>
            <label className="toggle">
              <input type="checkbox" checked={renaming.you} onChange={(e) => setRenaming({ ...renaming, you: e.currentTarget.checked })} />
              <span>this is me</span>
            </label>
            {!renaming.you && (
              <input
                className="input"
                autoFocus
                placeholder="name (empty to clear)"
                value={renaming.name}
                onChange={(e) => setRenaming({ ...renaming, name: e.currentTarget.value })}
                onKeyDown={(e) => { if (e.key === "Enter") void saveRename(); if (e.key === "Escape") setRenaming(null); }}
              />
            )}
            <div className="save-row">
              <button className="control primary" onClick={saveRename}>save</button>
              <button className="control" onClick={() => setRenaming(null)}>cancel</button>
            </div>
          </div>
        </Sheet>
      )}
      {insightsOpen && detail ? (
        <InsightsRail meetingId={id} meeting={detail.meeting} live={live} finishing={finishing} onMeetingChanged={load} onCopy={() => copySection("insights")} copied={copied === "insights"} onCollapse={() => setInsightsOpen(false)} />
      ) : !insightsOpen ? (
        <aside className="insights-rail-collapsed">
          <button className="ghost boxed" title="Show insights (Ctrl+])" onClick={() => setInsightsOpen(true)}>◂</button>
          <span className="vertical-label">insights</span>
        </aside>
      ) : null}
    </div>
  );
}

export function SectionHeader({ icon, title, onCopy, copied }: { icon: string; title: string; onCopy?: () => void; copied?: boolean }) {
  return (
    <div className="section-header">
      <span className="sh-icon">{icon}</span>
      <span className="sh-title">{title}</span>
      <span className="th-spacer" />
      {onCopy && <button className={`copy-btn ${copied ? "ok" : ""}`} onClick={onCopy}>{copied ? "✓ copied" : "⧉ copy"}</button>}
    </div>
  );
}

/** Turns per block; each block is a `content-visibility: auto` island so the browser
 *  skips layout and paint for off-screen stretches of a long transcript. */
const TURN_BLOCK = 40;

const Turn = memo(function Turn({ speaker, name, color, start, text, keys, trim, onRemove, onTrimBefore, onTrimAfter }: {
  speaker: number;
  name: string;
  color: string;
  start: number;
  text: string;
  /** Persisted line keys, joined; only changes when the turn's lines change. */
  keys: string;
  trim?: boolean;
  onRemove?: (keys: string[]) => void;
  onTrimBefore?: (startS: number) => void;
  onTrimAfter?: (startS: number) => void;
}) {
  return (
    <div className="turn" data-speaker={speaker}>
      <div className="turn-head" style={{ color }}>
        {name} · {ts(start)}
        {trim && (
          <span className="turn-tools">
            <button className="ghost tiny" onClick={() => onTrimBefore?.(start)}>⇤ cut before</button>
            <button className="ghost tiny" onClick={() => onRemove?.(keys.split("\u0001"))}>✕</button>
            <button className="ghost tiny" onClick={() => onTrimAfter?.(start)}>cut after ⇥</button>
          </span>
        )}
      </div>
      <div className="turn-text">{text}</div>
    </div>
  );
});

function TranscriptBody({ lines, label, isYou, trim, onRemove, onTrimBefore, onTrimAfter }: {
  lines: Line[];
  label: (id: number) => string;
  isYou: (id: number) => boolean;
  trim?: boolean;
  onRemove?: (keys: string[]) => void;
  onTrimBefore?: (startS: number) => void;
  onTrimAfter?: (startS: number) => void;
}) {
  // Group consecutive finals by speaker into turns; interims render as the
  // macOS TerminalInterimRow ("listening…" with a pulsing bar). Final turns are
  // memoized on primitive props, so an interim update re-renders one row, not
  // the whole transcript; blocks of turns are content-visibility islands.
  const turns: { speaker: number; lines: Line[]; interim: boolean }[] = [];
  for (const l of lines) {
    const last = turns[turns.length - 1];
    if (!l.final) { turns.push({ speaker: l.speaker, lines: [l], interim: true }); continue; }
    if (last && !last.interim && last.speaker === l.speaker) last.lines.push(l);
    else turns.push({ speaker: l.speaker, lines: [l], interim: false });
  }
  const blocks: typeof turns[] = [];
  for (let i = 0; i < turns.length; i += TURN_BLOCK) blocks.push(turns.slice(i, i + TURN_BLOCK));
  return (
    <div className="turns">
      {blocks.map((block, b) => (
        <div className="turn-block" key={block[0]?.lines[0]?.key ?? b}>
          {block.map((t, i) => {
            const color = speakerColor(t.speaker, isYou(t.speaker));
            if (t.interim) {
              const prev = i > 0 ? block[i - 1] : b > 0 ? blocks[b - 1][blocks[b - 1].length - 1] : undefined;
              const prevSame = prev?.speaker === t.speaker;
              return (
                <div className="interim-row" key={t.lines[0].key}>
                  {!prevSame && (
                    <div className="interim-head">
                      <span className="bar" style={{ background: color }} />
                      <span style={{ color }}>{label(t.speaker)}</span>
                      <span className="sep">•</span>
                      <span className="listening">listening...</span>
                    </div>
                  )}
                  <div className="interim-body"><span className="bar pulse" style={{ background: color }} /><span>{t.lines[0].text}</span></div>
                </div>
              );
            }
            return (
              <Turn
                key={`${t.speaker}-${t.lines[0].key}`}
                speaker={t.speaker}
                name={label(t.speaker)}
                color={color}
                start={t.lines[0].start}
                text={t.lines.map((l) => l.text).join(" ")}
                keys={t.lines.map((l) => l.key).join("\u0001")}
                trim={trim}
                onRemove={onRemove}
                onTrimBefore={onTrimBefore}
                onTrimAfter={onTrimAfter}
              />
            );
          })}
        </div>
      ))}
    </div>
  );
}
