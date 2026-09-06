import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  coachingOverview,
  errorMessage,
  hasBridge,
  lookupDocTopic,
  onInsightsStatus,
  onInsightsUpdated,
  onSalesSuggested,
  regenerateInsights,
  setSalesEnabled,
} from "../api";
import { fmt1, parseJsonArray, parseJsonObject } from "../format";
import { useTauriEvent } from "../useEvent";
import type { DocTopic, InsightsStatusEvent, Investigation, Meeting, TrainingMetrics } from "../types";

type Tab = "summary" | "questions" | "coaching" | "sales" | "playbook";

interface Props {
  meetingId: string;
  meeting: Meeting;
  live: boolean;
  finishing: boolean;
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
export function InsightsRail({ meetingId, meeting, live, finishing, onMeetingChanged }: Props) {
  const [tab, setTab] = useState<Tab>("summary");
  const [more, setMore] = useState(false);
  const [metrics, setMetrics] = useState<TrainingMetrics | null>(null);
  const [status, setStatus] = useState<Record<string, InsightsStatusEvent>>({});
  const [salesSuggested, setSalesSuggested] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useTauriEvent(onInsightsUpdated, (id) => {
    if (id === meetingId) void onMeetingChanged();
  });
  useTauriEvent(onInsightsStatus, (ev) => {
    if (ev.meeting_id !== meetingId) return;
    setStatus((prev) => ({ ...prev, [ev.mode]: ev }));
  });
  useTauriEvent(onSalesSuggested, (id) => {
    if (id === meetingId && !meeting.sales_enabled) setSalesSuggested(true);
  });

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
  const docs = parseJsonArray<{ topic: string; answer: string; citations?: { title: string; url?: string | null }[]; priority?: string }>(meeting.docs);
  const docTopics = parseJsonArray<DocTopic>(meeting.doc_topics);
  const investigations = parseJsonArray<Investigation>(meeting.investigations);
  const hasSales = Object.values(meddpicc).some((v) => v && v.trim());

  const standardErr = status.standard?.state === "error" ? status.standard.message : null;
  const running = Object.values(status).some((s) => s.state === "running");

  async function toggleSales(enabled: boolean) {
    setError(null);
    try {
      await setSalesEnabled(meetingId, enabled);
      setSalesSuggested(false);
      await onMeetingChanged();
      if (enabled) setTab("sales");
      if (enabled && !live) await regenerateInsights(meetingId).catch(() => {});
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  async function lookup(label: string) {
    setError(null);
    try {
      await lookupDocTopic(meetingId, label);
    } catch (e) {
      setError(errorMessage(e));
    }
  }

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
              <button onClick={() => (setTab("sales"), setMore(false))}>
                sales (MEDDPICC){meeting.sales_enabled ? " ✓" : hasSales ? " •" : ""}
              </button>
              <button onClick={() => (setTab("playbook"), setMore(false))}>playbook (docs)</button>
              {!live && (
                <button onClick={() => (setMore(false), regenerateInsights(meetingId).catch((e) => setError(errorMessage(e))))}>
                  regenerate insights
                </button>
              )}
            </div>
          )}
        </div>
      </div>

      <div className="insights-status">
        {finishing || running ? (
          <span className="muted small"><span className="dot dot-off pulse" /> {finishing ? "finishing insights…" : "updating…"}</span>
        ) : standardErr ? (
          <span className="small err">{standardErr}</span>
        ) : null}
      </div>
      {error && <div className="banner error">{error}</div>}
      {salesSuggested && !meeting.sales_enabled && (
        <div className="banner warn">
          This sounds like a sales conversation. Enable Sales analysis?
          <button className="ghost" onClick={() => toggleSales(true)}>enable</button>
          <button className="ghost" onClick={() => setSalesSuggested(false)}>dismiss</button>
        </div>
      )}

      <div className="insights-body">
        {tab === "summary" && (
          !meeting.summary && actionItems.length === 0 ? (
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
              {investigations.length > 0 && (
                <Section title="investigations" tone="topics">
                  {investigations.map((inv, i) => (
                    <div className="doc-card" key={i}>
                      <div className="doc-topic">{inv.focus}</div>
                      <p className="small">{inv.answer}</p>
                      {inv.sources?.map((s, j) => (
                        <a key={j} href="#" className="citation" onClick={(e) => (e.preventDefault(), openUrl(s.url))}>
                          {s.title}
                        </a>
                      ))}
                      {inv.referenced_files?.length > 0 && (
                        <p className="muted tiny">files: {inv.referenced_files.join(", ")}</p>
                      )}
                    </div>
                  ))}
                </Section>
              )}
            </>
          )
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
          <>
            <label className="toggle">
              <input type="checkbox" checked={meeting.sales_enabled} onChange={(e) => toggleSales(e.currentTarget.checked)} />
              <span>Sales analysis (MEDDPICC) for this meeting</span>
            </label>
            {!hasSales ? (
              <p className="muted">
                {meeting.sales_enabled
                  ? live ? "MEDDPICC appears after a few minutes of conversation." : "Nothing was extracted yet. Regenerate insights to run it on this transcript."
                  : "Opt in above and the next insights pass extracts qualification evidence."}
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
            )}
          </>
        )}

        {tab === "playbook" && (
          <>
            {docTopics.length === 0 && docs.length === 0 ? (
              <p className="muted">Set a Docs MCP URL in Settings. Topics the conversation raises appear here and can be looked up in your docs.</p>
            ) : (
              <>
                {docTopics.length > 0 && (
                  <div className="topics">
                    {docTopics.map((t) => (
                      <div className={`topic ${t.state}`} key={t.id}>
                        <span>{t.label}</span>
                        {t.state === "looking_up" ? (
                          <span className="muted tiny">looking up…</span>
                        ) : t.state === "answered" ? (
                          <span className="tiny ok">answered</span>
                        ) : t.state === "no_match" ? (
                          <span className="muted tiny">no match</span>
                        ) : (
                          <button className="ghost tiny" onClick={() => lookup(t.label)} title={t.error ?? ""}>
                            {t.state === "failed" ? "retry" : "look up"}
                          </button>
                        )}
                      </div>
                    ))}
                  </div>
                )}
                <div className="docs">
                  {docs.map((d, i) => (
                    <div className={`doc-card ${d.priority === "high" ? "high" : ""}`} key={i}>
                      <div className="doc-topic">{d.topic}</div>
                      <p>{d.answer}</p>
                      {d.citations?.map((c, j) =>
                        c.url ? (
                          <a key={j} href="#" className="citation" onClick={(e) => (e.preventDefault(), openUrl(c.url!))}>
                            {c.title}
                          </a>
                        ) : (
                          <span key={j} className="citation muted">{c.title}</span>
                        ),
                      )}
                    </div>
                  ))}
                </div>
              </>
            )}
          </>
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
