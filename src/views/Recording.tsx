import { useEffect, useRef, useState } from "react";
import { LevelMeter } from "../components/LevelMeter";
import {
  errorMessage,
  getLevels,
  hasBridge,
  onTranscript,
  onTranscriptionStatus,
  recordingStatus,
  startRecording,
  stopRecording,
} from "../api";
import { elapsed, streamStatusText } from "../format";
import { useTauriEvent } from "../useEvent";
import type { Levels, StreamStatus, TranscriptEventPayload } from "../types";

interface Line {
  key: string;
  speaker: number;
  label: string;
  text: string;
  source: string;
  final: boolean;
}

/**
 * Deepgram sends a stream of interim results that grow ("hel", "hello", "hello
 * th…") before a final replaces them. We keep at most one interim line per
 * source channel and replace it in place; finals are appended and the interim
 * for that channel is cleared.
 */
function applyEvent(lines: Line[], ev: TranscriptEventPayload, seq: number): Line[] {
  const interimKey = `interim:${ev.source}`;
  const withoutInterim = lines.filter((l) => l.key !== interimKey);
  const line: Line = {
    key: ev.is_final ? `final:${seq}` : interimKey,
    speaker: ev.speaker_id,
    label: ev.speaker_label,
    text: ev.text,
    source: ev.source,
    final: ev.is_final,
  };
  return [...withoutInterim, line];
}

export function Recording() {
  const [levels, setLevels] = useState<Levels>({ mic: 0, system: 0, recording: false });
  const [status, setStatus] = useState<StreamStatus | null>(null);
  const [elapsedS, setElapsedS] = useState(0);
  const [title, setTitle] = useState("New meeting");
  const [lines, setLines] = useState<Line[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedId, setSavedId] = useState<string | null>(null);
  const seq = useRef(0);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!hasBridge) return;
    const id = window.setInterval(() => {
      getLevels().then(setLevels).catch(() => {});
      recordingStatus()
        .then((s) => {
          setElapsedS(s.elapsed_seconds);
          if (s.stream) setStatus(s.stream);
        })
        .catch(() => {});
    }, 200);
    return () => window.clearInterval(id);
  }, []);

  useTauriEvent(onTranscript, (ev) => {
    seq.current += 1;
    setLines((prev) => applyEvent(prev, ev, seq.current));
  });
  useTauriEvent(onTranscriptionStatus, setStatus);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [lines]);

  async function toggle() {
    setBusy(true);
    setError(null);
    try {
      if (levels.recording) {
        const id = await stopRecording();
        setSavedId(id);
        setLines((prev) => prev.filter((l) => l.final));
      } else {
        setLines([]);
        setSavedId(null);
        setStatus(null);
        await startRecording(title);
      }
      setLevels(await getLevels());
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const statusText = streamStatusText(status, levels.recording);
  const statusTone =
    status?.state === "failed" || status?.state === "reconnecting"
      ? "dot-warn"
      : levels.recording && status?.state === "connected"
        ? "dot-ok"
        : "dot-off";

  return (
    <div className="view">
      <h1 className="view-title">Recording</h1>

      <div className="rec-controls">
        <input
          className="input"
          value={title}
          disabled={levels.recording}
          onChange={(e) => setTitle(e.currentTarget.value)}
          placeholder="Meeting title"
        />
        <button
          className={`btn ${levels.recording ? "danger" : "primary"}`}
          onClick={toggle}
          disabled={busy || !hasBridge}
        >
          {busy ? "…" : levels.recording ? "■ Stop" : "● Record"}
        </button>
        {levels.recording && <span className="timer">{elapsed(elapsedS)}</span>}
      </div>

      <div className="status-line">
        <span className={`dot ${statusTone}`} />
        <span className="muted">{statusText}</span>
      </div>

      {error && <div className="banner error">{error}</div>}
      {savedId && !levels.recording && (
        <div className="banner ok">Meeting saved. Find it in History.</div>
      )}

      <div className="meters">
        <LevelMeter label="mic" level={levels.mic} active={levels.recording} />
        <LevelMeter label="system" level={levels.system} active={levels.recording} />
      </div>

      <section className="card transcript">
        <h2 className="card-title">Live transcript</h2>
        {lines.length === 0 ? (
          <p className="muted">
            {levels.recording ? "Listening…" : "Press Record to begin."}
          </p>
        ) : (
          <div className="lines">
            {lines.map((l) => (
              <div className={`line ${l.final ? "" : "interim"}`} key={l.key}>
                <span className="spk">{l.label}</span>
                <span className="txt">{l.text}</span>
              </div>
            ))}
            <div ref={bottomRef} />
          </div>
        )}
      </section>
    </div>
  );
}
