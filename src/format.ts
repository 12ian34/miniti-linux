import type { Meeting, StreamStatus } from "./types";

/** Mirror of the Rust `Meeting::display_title`: strip legacy "YYYY-MM-DD HH:MM " prefixes. */
export function displayTitle(m: Pick<Meeting, "title">): string {
  let t = m.title.trim();
  const dated = /^\d{4}-\d{2}-\d{2}\s+(?:\d{1,2}:\d{2}\s+)?(.*)$/.exec(t);
  if (dated) t = dated[1].trim();
  return t.length > 0 ? t : "Untitled meeting";
}

export function when(ts: number | null): string {
  if (!ts) return "";
  return new Date(ts * 1000).toLocaleString();
}

export function duration(m: Pick<Meeting, "started_at" | "ended_at">): string {
  if (!m.started_at || !m.ended_at) return "";
  const s = Math.max(0, m.ended_at - m.started_at);
  const mm = Math.floor(s / 60);
  const ss = s % 60;
  return `${mm}:${ss.toString().padStart(2, "0")}`;
}

export function elapsed(seconds: number): string {
  const s = Math.floor(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  return h > 0
    ? `${h}:${m.toString().padStart(2, "0")}:${sec.toString().padStart(2, "0")}`
    : `${m}:${sec.toString().padStart(2, "0")}`;
}

/** Fallback label when the backend has not supplied one. */
export function fallbackSpeakerLabel(id: number): string {
  if (id === 1000) return "You";
  if (id >= 1000) return `Speaker ${id - 1000 + 1} (mic)`;
  return `Speaker ${id + 1}`;
}

export function streamStatusText(st: StreamStatus | null, recording: boolean): string {
  if (!recording) return "idle";
  if (!st) return "starting…";
  switch (st.state) {
    case "connecting":
      return "connecting to Deepgram…";
    case "connected":
      return "live";
    case "reconnecting":
      return `reconnecting (attempt ${st.attempt})…`;
    case "ended":
      return "finished";
    case "failed":
      return `transcription failed: ${st.reason}`;
  }
}

export function fmt1(n: number): string {
  return (Math.round(n * 10) / 10).toFixed(1);
}
