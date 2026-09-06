export type AppMode = "managed" | "byok";

export interface EnvHealth {
  app_name: string;
  app_version: string;
  platform: string;
  pcm_contract: string;
  os: string;
  tauri_bridge: boolean;
  microphone_available: boolean;
  system_audio_available: boolean;
  backend_key_present: boolean;
  device_id: string;
}

export interface Prefs {
  app_mode: AppMode;
  language: string;
  byok_deepgram_key: string | null;
  byok_openai_key: string | null;
  webhook_url: string | null;
  docs_mcp_url: string | null;
  export_folder: string | null;
  filler_overrides: string[];
  smart_meetings_enabled: boolean;
  show_tray: boolean;
  show_floating_indicator: boolean;
  capture_system_audio: boolean;
  accepted_terms_version: string | null;
  onboarding_complete: boolean;
  live_insights_enabled: boolean;
  sales_insights_default: boolean;
  codebase_root: string | null;
  personal_dictionary: string[];
  notifications_enabled: boolean;
  live_guidance_enabled: boolean;
  auto_stop_minutes: number;
  calendar_auto_start: boolean;
  calendar_auto_stop: boolean;
}

/** Smart-meeting decision shown on the floating surface / tray / in-app. */
export interface SmartPrompt {
  id: string;
  /** start | quiet | ending | calendar | clear */
  kind: "start" | "quiet" | "ending" | "calendar" | "clear";
  message: string;
  primary: string;
  secondary: string;
  tertiary?: string | null;
  meeting_id?: string | null;
  /** Seconds remaining for a countdown (ending grace / handoff). */
  countdown?: number | null;
}

export interface RecordingNudge {
  id: string;
  kind: "question" | "monologue" | "filler";
  title: string;
  message: string;
}

export interface DeepLinkEvent {
  scheme: string;
  host: string;
  query: Record<string, string>;
  url: string;
}

export interface Meeting {
  id: string;
  title: string;
  started_at: number | null;
  ended_at: number | null;
  language: string;
  notes: string;
  summary: string;
  action_items: string;
  key_decisions: string;
  topics: string;
  discussion_flow: string;
  suggested_questions: string;
  docs: string;
  speaker_names: string;
  self_speaker_ids: string;
  meddpicc: string;
  attendees: string;
  managed_session_id: string | null;
  calendar_event_id: string | null;
  import_source: string | null;
  pinned: boolean;
  insights_updated_at: number | null;
  sales_enabled: boolean;
  title_auto: boolean;
  doc_topics: string;
  manual_speaker_ids: string;
  investigations: string;
  created_at: number;
}

export interface InsightsStatusEvent {
  meeting_id: string;
  mode: string;
  state: "running" | "applied" | "degraded" | "error" | "finished";
  message: string | null;
}

export interface CatchUp {
  current_topic: string;
  questions_for_you: string[];
  recent_discussion: string[];
  key_decisions: string[];
}

export type InvestigationScope = "web" | "codebase";

export interface Investigation {
  focus: string;
  scope: InvestigationScope;
  answer: string;
  sources: { title: string; url: string }[];
  referenced_files: string[];
  at?: number;
}

export interface DocTopic {
  id: string;
  label: string;
  state: "pending" | "looking_up" | "answered" | "no_match" | "busy" | "failed";
  error?: string | null;
}

export interface TranscriptSegment {
  id: string;
  meeting_id: string;
  speaker: number;
  text: string;
  start_s: number;
  end_s: number;
  source: string;
}

export interface MeetingDetail {
  meeting: Meeting;
  segments: TranscriptSegment[];
  speaker_labels: Record<string, string>;
}

export interface Levels {
  mic: number;
  system: number;
  recording: boolean;
}

export type StreamStatus =
  | { state: "connecting" }
  | { state: "connected"; generation: number }
  | { state: "reconnecting"; attempt: number; reason: string }
  | { state: "ended" }
  | { state: "failed"; reason: string };

export interface RecordingStatus {
  recording: boolean;
  meeting_id: string | null;
  elapsed_seconds: number;
  stream: StreamStatus | null;
}

export type Gate = "force_update" | "terms" | "onboarding" | "main";

export interface LaunchGate {
  gate: Gate;
  current_version: string;
  min_version: string | null;
  latest_version: string | null;
  download_url: string | null;
  terms_version: string;
  backend_reachable: boolean;
  backend_error: string | null;
}

export interface Usage {
  minutes_used: number;
  minutes_limit: number | null;
  resets_at: string | null;
  tier: string | null;
  subscription_status: string | null;
  docs_lookups_used: number | null;
  docs_lookups_limit: number | null;
}

export interface FillerEntry {
  word: string;
  count: number;
}

export interface SpeakerStats {
  speaker_label: string;
  is_local_mic: boolean;
  word_count: number;
  segment_count: number;
  fillers: FillerEntry[];
  total_fillers: number;
  fillers_per_minute: number;
  words_per_minute: number;
  longest_monologue_words: number;
  questions_asked: number;
  avg_words_per_turn: number;
}

export interface TrainingMetrics {
  speakers: SpeakerStats[];
  talk_ratio_you: number;
  duration_minutes: number;
}

export type CoachingMetric =
  | "fillers"
  | "pace"
  | "clarity"
  | "questions"
  | "talk_ratio"
  | "monologue";

export interface CoachingMetricSummary {
  metric: CoachingMetric;
  status: "strong" | "balanced" | "focus";
  trend: "improving" | "steady" | "needs_attention" | "building_baseline";
  recent_value: string;
  previous_value: string | null;
  headline: string;
  observation: string;
  tip: string;
}

export interface CoachingReport {
  meeting_count: number;
  focus: CoachingMetricSummary;
  strengths: CoachingMetricSummary[];
  summaries: CoachingMetricSummary[];
}

export interface CoachingSnapshot {
  meeting_id: string;
  meeting_title: string;
  date: number;
  fillers_per_minute: number;
  words_per_minute: number;
  avg_words_per_turn: number;
  questions_per_30_minutes: number;
  talk_ratio: number | null;
  longest_monologue_words: number;
  top_filler: string | null;
}

export interface CoachingOverview {
  report: CoachingReport | null;
  snapshots: CoachingSnapshot[];
}

export interface TranscriptEventPayload {
  meeting_id: string;
  segment_id: string | null;
  text: string;
  speaker_id: number;
  speaker_label: string;
  start: number;
  end: number;
  is_final: boolean;
  confidence: number;
  source: string;
  channel_index: number | null;
}

export type CrmProvider = "attio" | "twenty";

export interface CalendarAttendee {
  email: string;
  displayName: string | null;
  responseStatus: string | null;
  organizer: boolean;
  self: boolean;
  domain: string | null;
}

export interface CalendarEvent {
  id: string;
  title: string;
  start: string;
  end: string;
  isAllDay: boolean;
  status: string | null;
  meetLink: string | null;
  conferenceUrl: string | null;
  attendees: CalendarAttendee[];
}

export interface CalendarView {
  connected: boolean;
  email: string | null;
  events: CalendarEvent[];
  upcoming: CalendarEvent[];
  error: string | null;
  available: boolean;
}

export interface CrmRecord {
  id: { workspace_id: string; object_id: string; record_id: string };
  record_text: string;
  record_image: string | null;
  object_slug: string;
  record_email: string | null;
  record_domain: string | null;
  record_detail: string | null;
}

export interface CrmTask {
  content: string;
  deadline_at?: string | null;
}
