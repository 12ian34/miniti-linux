import { useEffect, useState } from "react";
import { coachingOverview, hasBridge } from "../api";
import { fmt1, parseJsonArray, parseJsonObject } from "../format";
import type { Meeting, TrainingMetrics } from "../types";

type Tab = "summary" | "questions" | "coaching" | "sales" | "playbook";

interface Props {
  meetingId: string;
  meeting: Meeting;
  live: boolean;
  onMeetingChanged: () => Promise<void>;
}

interface Question {
  question: string;
  type: string;
  context?: string;
  priority?: string;
}

const MEDDPICC: [string, string, string][] = [
  ["metrics", "Metrics", "var(--meddpicc-m)"],
  ["economic_buyer", "Economic Buyer", "var(--meddpicc-e)"],
  ["decision_criteria", "Decision Criteria", "var(--meddpicc-d)"],
  ["decision_process", "Decision Process", "var(--meddpicc-d)"],
  ["paper_process", "Paper Process", "var(--meddpicc-p)"],
  ["identified_pain", "Identified Pain", "var(--meddpicc-i)"],
  ["champion", "Champion", "var(--meddpicc-c)"],
  ["competition", "Competition", "var(--meddpicc-c)"],
];

/**
 * The right-hand insights pane shared by live recording and saved detail:
 * lowercase summary / questions / coaching tabs, Sales and Playbook under More.
 */
export function InsightsRail({ meetingId, meeting, live }: Props) {
  const [tab, setTab] = useState<Tab>("summary");
  const [more, setMore] = useState(false);
  const [metrics, setMetrics] = useState<TrainingMetrics | null>(null);

  useEffect(() => {
    if (!hasBridge || tab !== "coaching") return;
    let alive = true;
    const fetch = () => coachingOverview(meetingId).then((m) => alive && setMetrics(m)).catch(() => {});
    fetch();
    const t = live ? window.setInterval(fetch, 10_000) : undefined;
    return () => {
      alive = false;
      if (t) window.clearInterval(t);
    };
  }, [tab, meetingId, live, meeting.speaker_names, meeting.self_speaker_ids]);

  const actionItems = parseJsonArray<string>(meeting.action_items);
  const decisions = parseJsonArray<string>(meeting.key_decisions);
  const topics = parseJsonArray<string>(meeting.topics);
  const flow = parseJsonArray<string>(meeting.discussion_flow);
  const questions = parseJsonArray<Question>(meeting.suggested_questions);
  const meddpicc = parseJsonObject<Record<string, string | null>>(meeting.meddpicc) ?? {};
  const docs = parseJsonArray<{ topic: string; answer: string; citations?: { title: string; url?: string }[] }>(meeting.docs);
  const hasSales = Object.values(meddpicc).some((v) => v && v.trim());

  return (
    <aside className="insights">
      <div className="tab-strip">
        {(["summary", "questions", "coaching"] as Tab[]).map((t) => (
          <button key={t} className={`tab ${tab === t ? "active" : ""}`} onClick={() => setTab(t)}>
            {t}
          </button>
        ))}
        <div className="more">
          <button className={`tab ${tab === "sales" || tab === "playbook" ? "active" : ""}`} onClick={() => setMore(!more)}>
            more ▾
          </button>
          {more && (
            <div className="menu" onMouseLeave={() => setMore(false)}>
              <button onClick={() => (setTab("sales"), setMore(false))}>sales (MEDDPICC){hasSales ? " •" : ""}</button>
              <button onClick={() => (setTab("playbook"), setMore(false))}>playbook (docs)</button>
            </div>
          )}
        </div>
      </div>

      <div className="insights-body">
        {tab === "summary" && (
          <>
            {!meeting.summary && actionItems.length === 0 ? (
              <p className="muted">
                {live ? "Insights appear after a few sentences." : "No insights for this meeting yet."}
              </p>
            ) : (
              <>
                <Section title="summary" tone="summary">
                  <p>{meeting.summary}</p>
                </Section>
                {flow.length > 0 && (
                  <Section title="discussion" tone="discussion">
                    <ol>{flow.map((x, i) => <li key={i}>{x}</li>)}</ol>
                  </Section>
                )}
                {actionItems.length > 0 && (
                  <Section title="actions" tone="actions">
                    <ul className="checks">{actionItems.map((x, i) => <li key={i}>{x}</li>)}</ul>
                  </Section>
                )}
                {decisions.length > 0 && (
                  <Section title="decisions" tone="actions">
                    <ul>{decisions.map((x, i) => <li key={i}>{x}</li>)}</ul>
                  </Section>
                )}
                {topics.length > 0 && (
                  <Section title="topics" tone="topics">
                    <div className="pills">{topics.map((t, i) => <span className="pill" key={i}>{t}</span>)}</div>
                  </Section>
                )}
              </>
            )}
          </>
        )}

        {tab === "questions" && (
          questions.length === 0 ? (
            <p className="muted">{live ? "Questions appear once the conversation has some substance." : "No questions for this meeting."}</p>
          ) : (
            <div className="questions">
              {questions.map((q, i) => (
                <div className={`question ${q.priority === "high" ? "high" : ""}`} key={i}>
                  <div className="q-type">{q.type?.replace("_", " ")}{q.priority === "high" ? " · high" : ""}</div>
                  <div className="q-text">{q.question}</div>
                  {q.context && <div className="q-context">{q.context}</div>}
                </div>
              ))}
            </div>
          )
        )}

        {tab === "coaching" && (
          !metrics || metrics.speakers.length === 0 ? (
            <p className="muted">Coaching metrics appear once someone has spoken.</p>
          ) : (
            <CoachingPane metrics={metrics} />
          )
        )}

        {tab === "sales" && (
          !hasSales ? (
            <p className="muted">
              Sales analysis (MEDDPICC) is opt-in. {live ? "It will start on the next insights pass once enabled in Settings." : "Nothing was extracted for this meeting."}
            </p>
          ) : (
            <div className="meddpicc">
              {MEDDPICC.map(([key, name, color]) => {
                const v = meddpicc[key];
                if (!v || !v.trim()) return null;
                return (
                  <div className="meddpicc-field" key={key}>
                    <div className="meddpicc-label" style={{ color }}>
                      <span className="letter" style={{ background: color }}>{name[0]}</span>
                      {name}
                    </div>
                    <ul>{v.split("\n").filter(Boolean).map((line, i) => <li key={i}>{line}</li>)}</ul>
                  </div>
                );
              })}
            </div>
          )
        )}

        {tab === "playbook" && (
          docs.length === 0 ? (
            <p className="muted">Configure a Docs MCP URL in Settings to get grounded playbook cards here.</p>
          ) : (
            <div className="docs">
              {docs.map((d, i) => (
                <div className="doc-card" key={i}>
                  <div className="doc-topic">{d.topic}</div>
                  <p>{d.answer}</p>
                  {d.citations?.map((c, j) => (
                    <a key={j} href={c.url ?? "#"} target="_blank" rel="noreferrer" className="citation">
                      {c.title}
                    </a>
                  ))}
                </div>
              ))}
            </div>
          )
        )}
      </div>
    </aside>
  );
}

function Section({ title, tone, children }: { title: string; tone: string; children: React.ReactNode }) {
  return (
    <section className={`insight insight-${tone}`}>
      <div className="insight-title">{title}</div>
      {children}
    </section>
  );
}

function CoachingPane({ metrics }: { metrics: TrainingMetrics }) {
  const you = metrics.speakers.find((s) => s.is_local_mic);
  const others = metrics.speakers.filter((s) => !s.is_local_mic);
  return (
    <div className="coaching-pane">
      <p className="muted small">
        {fmt1(metrics.duration_minutes)} min · you {Math.round(metrics.talk_ratio_you * 100)}% of words
      </p>
      {you && <SpeakerCard s={you} />}
      {others.length === 1 && <SpeakerCard s={others[0]} />}
      {others.length > 1 && (
        <details className="others">
          <summary>Others ({others.length})</summary>
          {others.map((s) => <SpeakerCard s={s} key={s.speaker_label} />)}
        </details>
      )}
    </div>
  );
}

function SpeakerCard({ s }: { s: TrainingMetrics["speakers"][number] }) {
  return (
    <div className="panel">
      <div className="panel-title">{s.speaker_label}</div>
      <div className="metric-rows">
        <MetricRow k="fillers" v={`${fmt1(s.fillers_per_minute)} / min`} color="var(--coach-fillers)" />
        <MetricRow k="pace" v={`${Math.round(s.words_per_minute)} wpm`} color="var(--coach-pace)" />
        <MetricRow k="clarity" v={`${fmt1(s.avg_words_per_turn)} words / turn`} color="var(--coach-clarity)" />
        <MetricRow k="questions" v={`${s.questions_asked}`} color="var(--coach-questions)" />
        <MetricRow k="monologue" v={`${s.longest_monologue_words} words`} color="var(--coach-monologue)" />
      </div>
      {s.fillers.length > 0 && (
        <p className="muted small">
          {s.fillers.slice(0, 5).map((f) => `${f.word} ×${f.count}`).join(" · ")}
        </p>
      )}
    </div>
  );
}

function MetricRow({ k, v, color }: { k: string; v: string; color: string }) {
  return (
    <div className="row-line">
      <span className="k" style={{ color }}>{k}</span>
      <span className="v">{v}</span>
    </div>
  );
}
