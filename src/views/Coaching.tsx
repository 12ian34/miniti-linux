import { useEffect, useState } from "react";
import { coachingOverview, coachingReport, errorMessage, hasBridge, listMeetings } from "../api";
import { displayTitle, fmt1, dateOnly } from "../format";
import { useStore } from "../store";
import type {
  CoachingMetric,
  CoachingMetricSummary,
  CoachingOverview,
  Meeting,
  TrainingMetrics,
} from "../types";

const METRIC_LABEL: Record<CoachingMetric, string> = {
  fillers: "fillers",
  pace: "pace",
  clarity: "clarity",
  questions: "questions",
  talk_ratio: "talk ratio",
  monologue: "monologue",
};

const METRIC_HELP: Record<CoachingMetric, string> = {
  fillers: "Filler words per minute you spoke. Deepgram under-reports hesitations, so treat this as a floor.",
  pace: "Your words per minute. 110–180 is a conversational range.",
  clarity: "Average words per turn. 5–20 keeps a point per turn.",
  questions: "Questions you asked per 30 minutes.",
  talk_ratio: "Share of all words that were yours. 35–65% is balanced for a two-way discussion.",
  monologue: "Longest uninterrupted run, in words.",
};

type Tab = "focus" | "stats" | "history";

export function Coaching() {
  const { navigate } = useStore();
  const [tab, setTab] = useState<Tab>("focus");
  const [overview, setOverview] = useState<CoachingOverview | null>(null);
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [selectedId, setSelectedId] = useState<string>("");
  const [metrics, setMetrics] = useState<TrainingMetrics | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!hasBridge) return;
    coachingReport().then(setOverview).catch((e) => setError(errorMessage(e)));
    listMeetings().then((m) => {
      setMeetings(m);
      if (m.length > 0) setSelectedId(m[0].id);
    });
  }, []);

  useEffect(() => {
    if (!selectedId) return;
    coachingOverview(selectedId)
      .then(setMetrics)
      .catch(() => setMetrics(null));
  }, [selectedId]);

  return (
    <div className="view">
      <h1 className="view-title">Coaching</h1>
      <p className="muted">Local metrics only — computed on this machine, never sent to the insights API.</p>
      {error && <div className="banner error">{error}</div>}

      <div className="tabs">
        {(["focus", "stats", "history"] as Tab[]).map((t) => (
          <button key={t} className={`tab ${tab === t ? "active" : ""}`} onClick={() => setTab(t)}>
            {t}
          </button>
        ))}
      </div>

      {tab === "focus" && <Focus overview={overview} />}
      {tab === "stats" && <Stats overview={overview} />}
      {tab === "history" && (
        <>
          {overview && overview.snapshots.length > 0 && (
            <div className="table-wrap">
              <table className="table">
                <thead>
                  <tr><th>meeting</th><th>date</th><th>fillers</th><th>pace</th><th>questions</th><th>talk</th></tr>
                </thead>
                <tbody>
                  {overview.snapshots.map((s) => (
                    <tr key={s.meeting_id} onClick={() => navigate({ kind: "meeting", id: s.meeting_id })}>
                      <td>{s.meeting_title}</td>
                      <td>{dateOnly(s.date)}</td>
                      <td>{fmt1(s.fillers_per_minute)}/min</td>
                      <td>{Math.round(s.words_per_minute)} wpm</td>
                      <td>{fmt1(s.questions_per_30_minutes)}</td>
                      <td>{s.talk_ratio == null ? "—" : `${Math.round(s.talk_ratio * 100)}%`}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <select
            className="input"
            value={selectedId}
            onChange={(e) => setSelectedId(e.currentTarget.value)}
          >
            {meetings.length === 0 && <option value="">No meetings</option>}
            {meetings.map((m) => (
              <option key={m.id} value={m.id}>
                {displayTitle(m)}
              </option>
            ))}
          </select>
          {metrics ? <MeetingStats metrics={metrics} /> : (
            <p className="muted">Select a meeting with a transcript to see its metrics.</p>
          )}
        </>
      )}
    </div>
  );
}

function Focus({ overview }: { overview: CoachingOverview | null }) {
  if (!overview?.report) {
    return <p className="muted">Record a meeting where you speak to see coaching guidance.</p>;
  }
  const r = overview.report;
  return (
    <>
      <section className="card">
        <h2 className="card-title">focus · {METRIC_LABEL[r.focus.metric]}</h2>
        <Summary s={r.focus} />
      </section>
      {r.strengths.length > 0 && (
        <section className="card">
          <h2 className="card-title">strengths</h2>
          <div className="rows">
            {r.strengths.map((s) => (
              <div key={s.metric}>
                <div className="row-line">
                  <span className="k">{METRIC_LABEL[s.metric]}</span>
                  <span className="v">{s.headline}</span>
                </div>
                <p className="muted">{s.observation}</p>
              </div>
            ))}
          </div>
        </section>
      )}
      <p className="muted">Based on your last {r.meeting_count} meeting{r.meeting_count === 1 ? "" : "s"}.</p>
    </>
  );
}

function Summary({ s }: { s: CoachingMetricSummary }) {
  return (
    <div className="rows">
      <div className="row-line">
        <span className={`dot ${s.status === "strong" ? "dot-ok" : s.status === "focus" ? "dot-warn" : "dot-off"}`} />
        <span className="v">{s.headline}</span>
      </div>
      <p className="muted">{s.observation}</p>
      <p>{s.tip}</p>
      <div className="row-line">
        <span className="k">recent</span>
        <span className="v">{s.recent_value}</span>
        {s.previous_value && (
          <>
            <span className="k">previous</span>
            <span className="v">{s.previous_value}</span>
          </>
        )}
        <span className="muted">{s.trend.replace("_", " ")}</span>
      </div>
    </div>
  );
}

function Stats({ overview }: { overview: CoachingOverview | null }) {
  if (!overview?.report) {
    return <p className="muted">No coaching data yet.</p>;
  }
  return (
    <div className="stat-list">
      {overview.report.summaries.map((s) => (
        <div className="stat" key={s.metric}>
          <div className="stat-head">
            <span className="k">{METRIC_LABEL[s.metric]}</span>
            <span className="metric-value">{s.recent_value}</span>
            <span className={`tone ${toneClass(s)}`}>
              {s.previous_value ? `was ${s.previous_value}` : "building baseline"}
            </span>
          </div>
          <p className="muted">{METRIC_HELP[s.metric]}</p>
        </div>
      ))}
    </div>
  );
}

function toneClass(s: CoachingMetricSummary): string {
  if (s.trend === "improving") return "good";
  if (s.trend === "needs_attention") return "bad";
  return "";
}

function MeetingStats({ metrics }: { metrics: TrainingMetrics }) {
  return (
    <>
      <p className="muted">
        {fmt1(metrics.duration_minutes)} min · you spoke {Math.round(metrics.talk_ratio_you * 100)}% of words
      </p>
      {metrics.speakers.map((sp) => (
        <section className="card" key={sp.speaker_label}>
          <h2 className="card-title">{sp.speaker_label}</h2>
          <div className="metric-grid">
            <Metric label="words" value={`${sp.word_count}`} />
            <Metric label="pace" value={`${Math.round(sp.words_per_minute)} wpm`} />
            <Metric label="fillers" value={`${fmt1(sp.fillers_per_minute)} / min`} />
            <Metric label="questions" value={`${sp.questions_asked}`} />
            <Metric label="longest monologue" value={`${sp.longest_monologue_words} words`} />
            <Metric label="words / turn" value={fmt1(sp.avg_words_per_turn)} />
          </div>
          {sp.fillers.length > 0 && (
            <p className="muted">
              top fillers: {sp.fillers.slice(0, 5).map((f) => `${f.word} ×${f.count}`).join(", ")}
            </p>
          )}
        </section>
      ))}
    </>
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
