import { useEffect, useRef, useState } from "react";
import { LevelMeter } from "../components/LevelMeter";
import {
  getLevels,
  hasBridge,
  onTranscript,
  startRecording,
  stopRecording,
} from "../api";
import type { Levels, TranscriptEventPayload } from "../types";

interface Line {
  key: string;
  speaker: number;
  text: string;
  source: string;
  final: boolean;
}

function speakerLabel(id: number): string {
  // Mic-space speakers are offset by 1000 (see deepgram::MIC_SPEAKER_OFFSET).
  if (id >= 1000) return `You ${id - 1000}`;
  return `Speaker ${id}`;
}

export function Recording() {
  const [levels, setLevels] = useState<Levels>({ mic: 0, system: 0, recording: false });
  const [title, setTitle] = useState("New meeting");
  const [lines, setLines] = useState<Line[]>([]);
  const [busy, setBusy] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!hasBridge) return;
    const id = window.setInterval(() => {
      getLevels().then(setLevels).catch(() => {});
    }, 200);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    if (!hasBridge) return;
    let unlisten: (() => void) | undefined;
    onTranscript((ev: TranscriptEventPayload) => {
      setLines((prev) => [
        ...prev,
        {
          key: `${ev.start}-${ev.speaker_id}-${prev.length}`,
          speaker: ev.speaker_id,
          text: ev.text,
          source: ev.source,
          final: ev.is_final,
        },
      ]);
    }).then((fn) => (unlisten = fn));
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [lines]);

  async function toggle() {
    setBusy(true);
    try {
      if (levels.recording) {
        await stopRecording();
      } else {
        setLines([]);
        await startRecording(title);
      }
      setLevels(await getLevels());
    } finally {
      setBusy(false);
    }
  }

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
          {levels.recording ? "■ Stop" : "● Record"}
        </button>
      </div>

      <div className="meters">
        <LevelMeter label="mic" level={levels.mic} active={levels.recording} />
        <LevelMeter label="system" level={levels.system} active={levels.recording} />
      </div>

      <section className="card transcript">
        <h2 className="card-title">Live transcript</h2>
        {lines.length === 0 ? (
          <p className="muted">
            {levels.recording
              ? "Listening… (transcript appears when a Deepgram credential is configured)"
              : "Press Record to begin."}
          </p>
        ) : (
          <div className="lines">
            {lines.map((l) => (
              <div className="line" key={l.key}>
                <span className="spk">{speakerLabel(l.speaker)}</span>
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
