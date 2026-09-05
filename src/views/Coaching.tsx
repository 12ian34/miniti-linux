import { useEffect, useState } from "react";
import { coachingOverview, hasBridge, listMeetings } from "../api";
import type { CoachingMetrics, Meeting } from "../types";

export function Coaching() {
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [selectedId, setSelectedId] = useState<string>("");
  const [metrics, setMetrics] = useState<CoachingMetrics | null>(null);

  useEffect(() => {
    if (!hasBridge) return;
    listMeetings().then((m) => {
      setMeetings(m);
      if (m.length > 0) setSelectedId(m[0].id);
    });
  }, []);

  useEffect(() => {
    if (!selectedId) return;
    coachingOverview(selectedId).then(setMetrics).catch(() => setMetrics(null));
  }, [selectedId]);

  return (
    <div className="view">
      <h1 className="view-title">Coaching</h1>
      <p className="muted">Local metrics only — never sent to the insights API.</p>

      <select
        className="input"
        value={selectedId}
        onChange={(e) => setSelectedId(e.currentTarget.value)}
      >
        {meetings.length === 0 && <option value="">No meetings</option>}
        {meetings.map((m) => (
          <option key={m.id} value={m.id}>
            {m.title || "Untitled meeting"}
          </option>
        ))}
      </select>

      {metrics ? (
        <div className="metric-grid">
          <Metric label="talk ratio" value={`${Math.round(metrics.talk_ratio * 100)}%`} />
          <Metric label="clarity" value={`${Math.round(metrics.clarity)}/100`} />
          <Metric label="pace" value={`${Math.round(metrics.pace_wpm)} wpm`} />
          <Metric label="fillers" value={`${metrics.filler_count}`} />
          <Metric label="questions" value={`${metrics.questions_asked}`} />
          <Metric
            label="longest monologue"
            value={`${Math.round(metrics.longest_monologue_s)}s`}
          />
          <Metric label="your words" value={`${metrics.self_words}`} />
          <Metric label="total words" value={`${metrics.total_words}`} />
        </div>
      ) : (
        <p className="muted">Select a meeting with transcript to see coaching metrics.</p>
      )}
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric">
      <div className="metric-value">{value}</div>
      <div className="metric-label">{label}</div>
    </div>
  );
}
