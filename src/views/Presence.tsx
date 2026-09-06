import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  hasBridge,
  onSmartPrompt,
  onRecordingNudge,
  onTranscriptionStatus,
  recordingStatus,
  showMainWindow,
  smartDecision,
  stopRecording,
} from "../api";
import { elapsed, streamStatusText } from "../format";
import { useTauriEvent } from "../useEvent";
import type { RecordingNudge, RecordingStatus, SmartPrompt, StreamStatus } from "../types";

/**
 * The floating recording surface (macOS NSPanel port): red dot + timer + stop,
 * expands for Smart-meeting decisions and live guidance nudges. Rendered in
 * its own always-on-top window; it never activates the main window.
 */
export function Presence() {
  const [status, setStatus] = useState<RecordingStatus>({ recording: false, meeting_id: null, elapsed_seconds: 0, stream: null });
  const [stream, setStream] = useState<StreamStatus | null>(null);
  const [prompt, setPrompt] = useState<SmartPrompt | null>(null);
  const [nudge, setNudge] = useState<RecordingNudge | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!hasBridge) return;
    const t = window.setInterval(() => recordingStatus().then(setStatus).catch(() => {}), 500);
    return () => window.clearInterval(t);
  }, []);
  useTauriEvent(onTranscriptionStatus, setStream);
  useTauriEvent(onSmartPrompt, (p) => setPrompt(p.kind === "clear" ? null : p));
  useTauriEvent(onRecordingNudge, (n) => {
    setNudge(n);
    window.setTimeout(() => setNudge((cur) => (cur?.id === n.id ? null : cur)), 10_000);
  });

  async function stop() {
    setBusy(true);
    try {
      await stopRecording();
    } finally {
      setBusy(false);
    }
  }

  const expanded = !!prompt || !!nudge;

  return (
    <div className={`presence ${expanded ? "expanded" : ""}`} data-tauri-drag-region onDoubleClick={() => showMainWindow()}>
      <div className="presence-row" data-tauri-drag-region>
        <span className={`dot ${status.recording ? "dot-rec pulse" : "dot-off"}`} />
        <span className="timer">{elapsed(status.elapsed_seconds)}</span>
        <span className="muted tiny ellipsis" data-tauri-drag-region>
          {streamStatusText(stream, status.recording)}
        </span>
        <span className="presence-spacer" data-tauri-drag-region />
        {status.recording && (
          <button className="btn recording small" onClick={stop} disabled={busy}>
            ■
          </button>
        )}
        <button className="ghost tiny" onClick={() => getCurrentWindow().hide()} title="Hide">
          ×
        </button>
      </div>
      {prompt && (
        <div className="presence-decision">
          <div className="small">{prompt.message}</div>
          <div className="presence-actions">
            <button className="btn primary small" onClick={() => smartDecision(prompt.id, "primary")}>{prompt.primary}</button>
            <button className="btn secondary small" onClick={() => smartDecision(prompt.id, "secondary")}>{prompt.secondary}</button>
            {prompt.tertiary && (
              <button className="ghost tiny" onClick={() => smartDecision(prompt.id, "tertiary")}>{prompt.tertiary}</button>
            )}
          </div>
        </div>
      )}
      {!prompt && nudge && (
        <div className={`presence-nudge ${nudge.kind}`}>
          <div className="small"><strong>{nudge.title}</strong> {nudge.message}</div>
        </div>
      )}
    </div>
  );
}
