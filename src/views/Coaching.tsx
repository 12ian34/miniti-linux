import { useEffect, useState } from "react";
import { coachingOverview, coachingReport, errorMessage, hasBridge, listMeetings } from "../api";
import { displayTitle, fmt1, dateOnly } from "../format";
import { useStore } from "../store";
import type {
  CoachingExample,
  CoachingMetric,
  CoachingMetricSummary,
  CoachingOverview,
  CoachingSnapshot,
  Meeting,
  TrainingMetrics,
} from "../types";

const METRIC_COLOR: Record<CoachingMetric, string> = {
  fillers: "var(--coach-fillers)",
  pace: "var(--coach-pace)",
  clarity: "var(--coach-clarity)",
  questions: "var(--coach-questions)",
  talk_ratio: "var(--coach-talk)",
  monologue: "var(--coach-monologue)",
};

const METRIC_UNIT: Record<CoachingMetric, string> = {
  fillers: "per min",
  pace: "wpm",
  clarity: "words / turn",
  questions: "per 30 min",
  talk_ratio: "% you",
  monologue: "words",
};

/** Broad conversational ranges shown as a faint band (matches the advisor's thresholds). */
const METRIC_BAND: Partial<Record<CoachingMetric, [number, number]>> = {
  pace: [110, 180],
  clarity: [5, 20],
  talk_ratio: [35, 65],
};

function metricValue(metric: CoachingMetric, s: CoachingSnapshot): number | null {
  switch (metric) {
    case "fillers": return s.fillers_per_minute;
    case "pace": return s.words_per_minute;
    case "clarity": return s.avg_words_per_turn;
    case "questions": return s.questions_per_30_minutes;
    case "talk_ratio": return s.talk_ratio == null ? null : s.talk_ratio * 100;
    case "monologue": return s.longest_monologue_words;
  }
}

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
  const { navigate } = useStore();
  if (!overview?.report) {
    return <p className="muted">Record a meeting where you speak to see coaching guidance.</p>;
  }
  const r = overview.report;
  const focusExamples = overview.examples.filter((e) => e.metric === r.focus.metric);
  const otherExamples = overview.examples.filter((e) => e.metric !== r.focus.metric);
  return (
    <>
      <section className="card">
        <h2 className="card-title">focus · {METRIC_LABEL[r.focus.metric]}</h2>
        <Summary s={r.focus} />
        {focusExamples.length > 0 && <Examples items={focusExamples} onOpen={(id) => navigate({ kind: "meeting", id })} />}
      </section>
      {otherExamples.length > 0 && (
        <section className="card">
          <h2 className="card-title">from your recent meetings</h2>
          <Examples items={otherExamples} onOpen={(id) => navigate({ kind: "meeting", id })} />
        </section>
      )}
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
  // Snapshots arrive newest first; charts read left to right in meeting order.
  const series = [...overview.snapshots].reverse();
  return (
    <>
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
      {overview.report.summaries.map((s) => (
        <TrendChart key={s.metric} metric={s.metric} series={series} />
      ))}
    </>
  );
}

/**
 * One card per metric (macOS Coaching overview): categorical meeting index on x
 * so every meeting gets equal spacing, quiet dashed grid, low-opacity area fade,
 * ringed latest point, numeric y labels, and a faint band for the broad range.
 */
function TrendChart({ metric, series }: { metric: CoachingMetric; series: CoachingSnapshot[] }) {
  const points = series.map((s, i) => ({ i, v: metricValue(metric, s), s }));
  const present = points.filter((p) => p.v != null) as { i: number; v: number; s: CoachingSnapshot }[];
  if (present.length === 0) {
    return (
      <section className="chart-card">
        <div className="chart-head"><span className="k">{METRIC_LABEL[metric]}</span><span className="unit">{metric === "talk_ratio" ? "needs a meeting with another speaker" : "no data yet"}</span></div>
      </section>
    );
  }
  const W = 560, H = 120, padL = 34, padR = 12, padT = 10, padB = 22;
  const n = Math.max(points.length, 2);
  const band = METRIC_BAND[metric];
  const values = present.map((p) => p.v).concat(band ?? []);
  let min = Math.min(0, ...values);
  let max = Math.max(...values);
  if (max === min) max = min + 1;
  const pad = (max - min) * 0.1;
  min = Math.max(metric === "talk_ratio" ? 0 : min - pad, 0);
  max = metric === "talk_ratio" ? Math.max(max + pad, 100) : max + pad;
  const x = (i: number) => padL + (i / (n - 1)) * (W - padL - padR);
  const y = (v: number) => padT + (1 - (v - min) / (max - min)) * (H - padT - padB);
  const path = present.map((p, k) => `${k === 0 ? "M" : "L"}${x(p.i).toFixed(1)},${y(p.v).toFixed(1)}`).join(" ");
  const area = `${path} L${x(present[present.length - 1].i).toFixed(1)},${y(min).toFixed(1)} L${x(present[0].i).toFixed(1)},${y(min).toFixed(1)} Z`;
  const ticks = [min, (min + max) / 2, max];
  const fmt = (v: number) => (max - min > 20 ? Math.round(v).toString() : v.toFixed(1));
  const last = present[present.length - 1];
  const color = METRIC_COLOR[metric];
  const everyLabel = points.length <= 8 ? 1 : Math.ceil(points.length / 6);
  return (
    <section className="chart-card" aria-label={`${METRIC_LABEL[metric]} trend: latest ${fmt(last.v)} ${METRIC_UNIT[metric]} on ${dateOnly(last.s.date)} across ${present.length} meetings`}>
      <div className="chart-head">
        <span className="k">{METRIC_LABEL[metric]}</span>
        <span className="latest" style={{ color }}>{fmt(last.v)}</span>
        <span className="unit">{METRIC_UNIT[metric]} · {dateOnly(last.s.date)}</span>
      </div>
      <svg className="chart" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img">
        {band && <rect className="band" x={padL} y={y(Math.min(band[1], max))} width={W - padL - padR} height={Math.max(0, y(Math.max(band[0], min)) - y(Math.min(band[1], max)))} />}
        {ticks.map((t) => (
          <g key={t}>
            <line className="grid" x1={padL} x2={W - padR} y1={y(t)} y2={y(t)} />
            <text className="axis" x={padL - 4} y={y(t) + 3} textAnchor="end">{fmt(t)}</text>
          </g>
        ))}
        <path d={area} fill={color} opacity={0.12} />
        <path d={path} fill="none" stroke={color} strokeWidth={1.5} />
        {present.map((p) => <circle key={p.i} cx={x(p.i)} cy={y(p.v)} r={2} fill={color} />)}
        <circle cx={x(last.i)} cy={y(last.v)} r={4.5} fill="none" stroke={color} strokeWidth={1.5} />
        {points.map((p) => (p.i % everyLabel === 0 || p.i === points.length - 1) && (
          <text key={p.i} className="xlabel" x={x(p.i)} y={H - 8}>{p.i + 1}</text>
        ))}
        <text className="xlabel" x={(padL + W - padR) / 2} y={H - 0.5}>meeting</text>
      </svg>
    </section>
  );
}

/** Clickable source passages (label + quote left, meeting title above date right). */
function Examples({ items, onOpen }: { items: CoachingExample[]; onOpen: (id: string) => void }) {
  return (
    <div className="example-list">
      {items.map((e) => (
        <button key={`${e.metric}-${e.meeting_id}`} className="example" onClick={() => onOpen(e.meeting_id)} title="open meeting">
          <div className="ex-main">
            <span className="ex-label" style={{ color: METRIC_COLOR[e.metric] }}>{e.label}</span>
            <span className="ex-quote">“{e.quote}”</span>
          </div>
          <div className="ex-meta">
            <span className="t">{e.meeting_title}</span>
            <span>{dateOnly(e.date)}</span>
          </div>
        </button>
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
