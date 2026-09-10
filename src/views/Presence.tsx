import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalPosition } from "@tauri-apps/api/window";
import {
  disableNudgeKind,
  hasBridge,
  onPresence,
  onPresenceAttention,
  onPresenceSettle,
  onRecordingNudge,
  onSmartPrompt,
  recordingPresence,
  setSalesEnabled,
  showMainWindow,
  smartDecision,
  stopRecording,
} from "../api";
import { useTauriEvent } from "../useEvent";
import type { RecordingNudge, RecordingPresence, SmartPrompt } from "../types";

const COLLAPSED_W = 236;
const EXPANDED_W = 360;
/** A nudge that nobody touches leaves on its own (macOS clears them after a while). */
const NUDGE_TTL_MS = 20_000;

const EMPTY: RecordingPresence = {
  is_recording: false, elapsed_seconds: 0, elapsed_text: "00:00", meeting_id: null, meeting_title: "",
  lifecycle_status: null, transcription_status: "Transcription: off", audio_status: "Audio: healthy",
  audio_health: "healthy", stream_state: null, grace_remaining_seconds: null, grace_app_name: null, call_app_name: null,
};

/** Short header title per prompt kind (macOS `SmartMeetingPrompt.title`). */
function promptTitle(p: SmartPrompt): string {
  switch (p.kind) {
    case "start": return "call detected";
    case "quiet": return "quiet room";
    case "ending": return p.countdown != null ? `ending ${p.countdown}s` : "call ended";
    case "calendar": return "next meeting";
    default: return p.message;
  }
}

function promptGlyph(p: SmartPrompt): string {
  switch (p.kind) {
    case "start": return "☎";
    case "quiet": return "☾";
    case "ending": return "■";
    case "calendar": return "▦";
    default: return "◆";
  }
}

function nudgeGlyph(n: RecordingNudge): string {
  switch (n.kind) {
    case "question": return "?";
    case "monologue": return "◉";
    case "filler": return "≈";
    case "sales": return "$";
  }
}

/**
 * Floating recording surface (macOS `RecordingIndicatorView`). Collapsed: glyph,
 * timer (or the prompt / nudge title, or the ending countdown), call app name
 * and a disclosure chevron. Expanded: meeting title and call / transcription /
 * audio status with controls, or the pending decision with its kind-specific
 * actions, or the nudge with dismiss / don't-remind. Sizes to its content; the
 * window never takes focus; position is remembered where the compositor allows.
 */
export function Presence() {
  const [presence, setPresence] = useState<RecordingPresence>(EMPTY);
  const [prompt, setPrompt] = useState<SmartPrompt | null>(null);
  const [nudge, setNudge] = useState<RecordingNudge | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [autoExpanded, setAutoExpanded] = useState(false);
  const [busy, setBusy] = useState(false);
  const dragged = useRef(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const prevRef = useRef({ recording: false, ending: false, promptId: null as string | null, nudgeId: null as string | null });

  // Presence model: pushed every second by the shell ticker; fetch once at mount.
  useEffect(() => {
    if (!hasBridge) return;
    recordingPresence().then(setPresence).catch(() => {});
  }, []);
  useTauriEvent(onPresence, setPresence);
  useTauriEvent(onSmartPrompt, (p) => setPrompt(p.kind === "clear" ? null : p));
  useTauriEvent(onRecordingNudge, (n) => {
    setNudge(n);
    window.setTimeout(() => setNudge((cur) => (cur?.id === n.id ? null : cur)), NUDGE_TTL_MS);
  });
  useTauriEvent(onPresenceAttention, () => { setExpanded(true); setAutoExpanded(true); });
  useTauriEvent(onPresenceSettle, () => { if (autoExpanded) { setExpanded(false); setAutoExpanded(false); } });

  // Attention policy (macOS `RecordingIndicatorAttentionPolicy.shouldAutoExpand`):
  // a new prompt, a new nudge, recording start, or the ending countdown starting
  // expands the surface; when the reason clears, an auto-expanded surface collapses.
  useEffect(() => {
    const prev = prevRef.current;
    const ending = presence.grace_remaining_seconds != null;
    const cur = { recording: presence.is_recording, ending, promptId: prompt?.id ?? null, nudgeId: nudge?.id ?? null };
    const attention =
      (cur.promptId && cur.promptId !== prev.promptId) ||
      (cur.nudgeId && cur.nudgeId !== prev.nudgeId) ||
      (cur.recording && !prev.recording) ||
      (cur.ending && !prev.ending);
    if (attention) {
      setExpanded(true);
      setAutoExpanded(true);
    } else if (autoExpanded && !cur.promptId && !cur.nudgeId && !cur.ending && prev.promptId !== prev.nudgeId) {
      // A prompt or nudge just cleared.
      if ((prev.promptId && !cur.promptId) || (prev.nudgeId && !cur.nudgeId)) {
        setExpanded(false);
        setAutoExpanded(false);
      }
    }
    prevRef.current = cur;
  }, [presence.is_recording, presence.grace_remaining_seconds, prompt?.id, nudge?.id, autoExpanded]);

  // Restore / persist position (macOS saves the panel origin in UserDefaults).
  useEffect(() => {
    if (!hasBridge) return;
    const win = getCurrentWindow();
    try {
      const raw = localStorage.getItem("presence.pos");
      if (raw) {
        const [x, y] = JSON.parse(raw) as [number, number];
        if (Number.isFinite(x) && Number.isFinite(y)) void win.setPosition(new LogicalPosition(x, y));
      }
    } catch { /* ignore */ }
    let un: (() => void) | undefined;
    win.onMoved(({ payload }) => {
      dragged.current = true;
      try {
        win.scaleFactor().then((s) => localStorage.setItem("presence.pos", JSON.stringify([payload.x / s, payload.y / s])));
      } catch { /* ignore */ }
    }).then((f) => (un = f));
    return () => un?.();
  }, []);

  // Size the window to the rendered content (the NSPanel sizes to its ideal size).
  useEffect(() => {
    const el = rootRef.current;
    if (!el || !hasBridge) return;
    const apply = () => {
      const height = Math.ceil(el.scrollHeight) + 2;
      invoke("resize_presence", { width: expanded ? EXPANDED_W : COLLAPSED_W, height }).catch(() => {});
    };
    apply();
    const ro = new ResizeObserver(apply);
    ro.observe(el);
    return () => ro.disconnect();
  }, [expanded, prompt, nudge, presence.meeting_title, presence.lifecycle_status, presence.grace_remaining_seconds]);

  async function run(fn: () => Promise<unknown>) {
    setBusy(true);
    try { await fn(); } catch { /* surfaced by the main window */ } finally { setBusy(false); }
  }
  function dismissNudge() { setNudge(null); }
  async function dontRemind(kind: string) { await run(() => disableNudgeKind(kind)); setNudge(null); }
  async function enableSales(n: RecordingNudge) {
    const id = n.meeting_id ?? presence.meeting_id;
    if (id) await run(() => setSalesEnabled(id, true));
    setNudge(null);
  }

  const ending = presence.grace_remaining_seconds;
  const warn = !!prompt || !!nudge;
  const glyph = prompt ? promptGlyph(prompt) : nudge ? nudgeGlyph(nudge) : ending != null ? "■" : "●";
  const title = prompt ? promptTitle(prompt) : nudge ? nudge.title : ending != null ? `ending ${ending}s` : presence.is_recording ? presence.elapsed_text : "not recording";
  const showApp = !prompt && !nudge && ending == null && presence.call_app_name;
  const toggle = () => { if (!dragged.current) { setExpanded((v) => !v); setAutoExpanded(false); } };

  return (
    <div ref={rootRef} className={`presence ${expanded ? "open" : ""}`} data-tauri-drag-region>
      <button
        className="presence-row"
        data-tauri-drag-region
        onMouseDown={() => (dragged.current = false)}
        onClick={toggle}
        onDoubleClick={() => showMainWindow()}
        title={expanded ? "Collapse meeting controls" : "Expand meeting controls"}
      >
        <span className={`glyph ${warn ? "warn" : "rec"}`} data-tauri-drag-region>{glyph}</span>
        <span className="presence-title ellipsis" data-tauri-drag-region>{title}</span>
        {showApp && <span className="presence-app ellipsis" data-tauri-drag-region>{presence.call_app_name}</span>}
        {presence.stream_state === "reconnecting" && !prompt && !nudge && <span className="muted tiny">reconnecting…</span>}
        <span className="presence-spacer" data-tauri-drag-region />
        <span className="chev-box">{expanded ? "▴" : "▾"}</span>
      </button>

      {expanded && (
        <div className="presence-body">
          {prompt ? (
            <>
              {prompt.kind === "start" ? (
                <div className="presence-message">take notes with <span className="brand-inline"><span className="hex">⬢</span> miniti</span></div>
              ) : (
                <div className="presence-message muted">
                  {prompt.message}{prompt.kind === "calendar" && prompt.countdown != null ? ` Starting the next recording in ${prompt.countdown}s.` : ""}
                </div>
              )}
              <div className="presence-actions">
                <button
                  className={`control ${prompt.kind === "start" || prompt.kind === "calendar" ? "positive" : "recording"} emph`}
                  disabled={busy}
                  onClick={() => run(() => smartDecision(prompt.id, "primary"))}
                >
                  {prompt.primary}
                </button>
                {prompt.join_url && (prompt.kind === "start" || prompt.kind === "calendar") && (
                  <button className="control positive" disabled={busy} title={prompt.join_url} onClick={() => run(() => smartDecision(prompt.id, "join"))}>
                    {prompt.kind === "calendar" ? "join next" : "join & take notes"}
                  </button>
                )}
                <button className="control" disabled={busy} onClick={() => run(() => smartDecision(prompt.id, "secondary"))}>{prompt.secondary}</button>
                {prompt.tertiary && <button className="ghost" disabled={busy} onClick={() => run(() => smartDecision(prompt.id, "tertiary"))}>{prompt.tertiary}</button>}
              </div>
            </>
          ) : nudge ? (
            <>
              <div className={`nudge ${nudge.kind}`}>{nudge.message}</div>
              <div className="presence-actions">
                {nudge.kind === "sales" && (
                  <button className="control primary emph" disabled={busy} onClick={() => enableSales(nudge)}>enable sales analysis</button>
                )}
                <button className="control" onClick={dismissNudge}>dismiss</button>
                <button className="control quiet" disabled={busy} onClick={() => dontRemind(nudge.kind)}>don't remind me</button>
              </div>
            </>
          ) : (
            <>
              {presence.meeting_title && <div className="presence-meeting ellipsis">{presence.meeting_title}</div>}
              {presence.lifecycle_status && <div className="presence-status">{presence.lifecycle_status}</div>}
              <div className="presence-status">{presence.transcription_status}</div>
              <div className={`presence-status ${presence.audio_health !== "healthy" ? "warn" : ""}`}>{presence.audio_status}</div>
              <div className="presence-actions">
                {ending != null ? (
                  <>
                    <button className="control positive emph" disabled={busy} onClick={() => run(() => smartDecision("ending-grace", "secondary"))}>keep recording</button>
                    <button className="control recording" disabled={busy} onClick={() => run(() => smartDecision("ending-grace", "primary"))}>end now</button>
                  </>
                ) : (
                  <>
                    <button className="control primary emph" onClick={() => showMainWindow()}>open meeting</button>
                    {presence.is_recording && <button className="control recording" disabled={busy} onClick={() => run(stopRecording)}>end meeting</button>}
                  </>
                )}
                <button className="ghost" onClick={() => getCurrentWindow().hide()}>hide</button>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
