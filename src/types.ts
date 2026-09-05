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
  suggested_questions: string;
  speaker_names: string;
  meddpicc: string;
  attendees: string;
  managed_session_id: string | null;
  calendar_event_id: string | null;
  import_source: string | null;
  pinned: boolean;
  insights_updated_at: number | null;
  created_at: number;
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

export interface Levels {
  mic: number;
  system: number;
  recording: boolean;
}

export interface CoachingMetrics {
  talk_ratio: number;
  self_words: number;
  total_words: number;
  filler_count: number;
  filler_rate: number;
  pace_wpm: number;
  longest_monologue_s: number;
  questions_asked: number;
  clarity: number;
}

export interface TranscriptEventPayload {
  text: string;
  speaker_id: number;
  start: number;
  end: number;
  is_final: boolean;
  source: string;
}
