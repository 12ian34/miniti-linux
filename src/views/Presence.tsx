import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalPosition } from "@tauri-apps/api/window";
import {
  hasBridge,
  onRecordingNudge,
  onSmartPrompt,
  recordingStatus,
  showMainWindow,
  smartDecision,
  stopRecording,
} from "../api";
import { elapsed } from "../format";
import { useTauriEvent } from "../useEvent";
import type { RecordingNudge, RecordingStatus, SmartPrompt } from "../types";

const COLLAPSED = { w: 236, h: 52 };
const EXPANDED = { w: 360, h: 150 };

/**
 * Floating recording surface (macOS `RecordingIndicatorView`): a compact row
 * with the recording glyph, elapsed time and a disclosure chevron; expands for
 * Smart-meeting decisions, live guidance and the stop control. Draggable; its
 * position is remembered across launches.
 */
export function Presence() {
  const [status, setStatus] = useState<RecordingStatus>({ recording: false, meeting_id: null, elapsed_seconds: 0, stream: null });
  const [prompt, setPrompt] = useState<SmartPrompt | null>(null);
  const [nudge, setNudge] = useState<RecordingNudge | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState(false);
  const dragged = useRef(false);

  useEffect(() => {
    if (!hasBridge) return;
    const t = window.setInterval(() => recordingStatus().then(setStatus).catch(() => {}), 500);
    return () => window.clearInterval(t);
  }, []);
  useTauriEvent(onSmartPrompt, (p) => setPrompt(p.kind === "clear" ? null : p));
  useTauriEvent(onRecordingNudge, (n) => {
    setNudge(n);
    window.setTimeout(() => setNudge((cur) => (cur?.id === n.id ? null : cur)), 10_000);
  });

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

  const open = expanded || !!prompt || !!nudge;
  useEffect(() => {
    const size = open ? EXPANDED : COLLAPSED;
    invoke("resize_presence", { width: size.w, height: size.h }).catch(() => {});
  }, [open]);

  async function stop() {
    setBusy(true);
    try { await stopRecording(); } finally { setBusy(false); }
  }

  const title = prompt ? prompt.message : nudge ? nudge.title : status.recording ? elapsed(status.elapsed_seconds) : "not recording";
  const warn = !!prompt || !!nudge;

  return (
    <div className={`presence ${open ? "open" : ""}`} data-tauri-drag-region>
      <button
        className="presence-row"
        data-tauri-drag-region
        onMouseDown={() => (dragged.current = false)}
        onClick={() => { if (!dragged.current) setExpanded((v) => !v); }}
        onDoubleClick={() => showMainWindow()}
      >
        <span className={`glyph ${warn ? "warn" : "rec"}`} data-tauri-drag-region>{warn ? "◆" : "●"}</span>
        <span className="presence-title ellipsis" data-tauri-drag-region>{title}</span>
        {status.stream?.state === "reconnecting" && <span className="muted tiny">reconnecting…</span>}
        <span className="presence-spacer" data-tauri-drag-region />
        <span className="chev-box">{open ? "▴" : "▾"}</span>
      </button>

      {open && (
        <div className="presence-body">
          {prompt ? (
            <>
              {prompt.countdown != null && <div className="muted tiny">{prompt.countdown}s</div>}
              <div className="presence-actions">
                <button className="control primary" onClick={() => smartDecision(prompt.id, "primary")}>{prompt.primary}</button>
                <button className="control" onClick={() => smartDecision(prompt.id, "secondary")}>{prompt.secondary}</button>
                {prompt.tertiary && <button className="ghost" onClick={() => smartDecision(prompt.id, "tertiary")}>{prompt.tertiary}</button>}
              </div>
            </>
          ) : nudge ? (
            <div className={`nudge ${nudge.kind}`}>{nudge.message}</div>
          ) : (
            <div className="presence-actions">
              {status.recording && <button className="control recording" onClick={stop} disabled={busy}>■ stop</button>}
              <button className="control" onClick={() => showMainWindow()}>open miniti</button>
              <button className="ghost" onClick={() => getCurrentWindow().hide()}>hide</button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
