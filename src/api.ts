import { invoke as tauriInvoke, type InvokeArgs } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  CalendarView,
  CatchUp,
  CoachingOverview,
  CrmProvider,
  CrmRecord,
  CrmTask,
  DeepLinkEvent,
  RecordingNudge,
  RecordingPresence,
  SmartPrompt,
  InsightsStatusEvent,
  Investigation,
  InvestigationScope,
  EnvHealth,
  AuthStatus,
  AuthDevices,
  LaunchGate,
  Levels,
  Meeting,
  MeetingDetail,
  Prefs,
  RecordingStatus,
  StreamStatus,
  TrainingMetrics,
  TranscriptEventPayload,
  TranscriptSegment,
  Usage,
} from "./types";

/** True when running inside the Tauri shell (vs a plain browser preview). */
export const hasBridge =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Longest a command may take before the UI gives up on it. Investigations and
 * insights can legitimately run for over a minute; nothing should take two.
 */
export const INVOKE_DEADLINE_MS = 120_000;

/**
 * `invoke` with a deadline. A command whose task dies (panic, runtime deadlock)
 * never settles its promise, which left the 0.3.0 enrollment button on
 * "Creating…" forever. Rejecting after the deadline turns that into an error
 * the user can see and report.
 */
function invoke<T>(cmd: string, args?: InvokeArgs, deadlineMs = INVOKE_DEADLINE_MS): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`miniti stopped responding (${cmd}). Restart the app; if it repeats, send the log from Settings → Privacy & Support.`)),
      deadlineMs,
    );
    tauriInvoke<T>(cmd, args).then(
      (v) => { clearTimeout(timer); resolve(v); },
      (e) => { clearTimeout(timer); reject(e); },
    );
  });
}

/** Normalize a rejected invoke into a readable message. */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}

export const environmentHealth = () => invoke<EnvHealth>("environment_health");
export const getPrefs = () => invoke<Prefs>("get_prefs");
export const setPrefs = (prefs: Prefs) => invoke<void>("set_prefs", { prefs });
export const getDeviceId = () => invoke<string>("get_device_id");

// Device-bound account (managed mode).
export const authStatus = () => invoke<AuthStatus>("auth_status");
export const authCreateAccount = () => invoke<string>("auth_create_account");
export const authRestoreAccount = (recoveryKey: string) => invoke<void>("auth_restore_account", { recoveryKey });
export const authRecoveryKey = () => invoke<string>("auth_recovery_key");
export const authRotateRecoveryKey = () => invoke<string>("auth_rotate_recovery_key");
export const authDevices = () => invoke<AuthDevices>("auth_devices");
export const authRemoveDevice = (installationId: string) => invoke<void>("auth_remove_device", { installationId });
export const authSignOut = () => invoke<void>("auth_sign_out");
export const authDeleteAccount = () => invoke<void>("auth_delete_account");

export const launchGate = () => invoke<LaunchGate>("launch_gate");
export const acceptTerms = () => invoke<Prefs>("accept_terms");
export const completeOnboarding = () => invoke<Prefs>("complete_onboarding");

export const getUsage = () => invoke<Usage>("get_usage");
export const subscribeUrl = () => invoke<string>("subscribe_url");
export const portalUrl = () => invoke<string>("portal_url");
export const restoreLicense = (licenseKey: string) =>
  invoke<unknown>("restore_license", { licenseKey });

export const startRecording = (title: string) =>
  invoke<string>("start_recording", { title });
export const stopRecording = () => invoke<string | null>("stop_recording");
export const getLevels = () => invoke<Levels>("get_levels");
export const recordingStatus = () => invoke<RecordingStatus>("recording_status");
export const recordingPresence = () => invoke<RecordingPresence>("recording_presence");
export const disableNudgeKind = (kind: string) => invoke<Prefs>("disable_nudge_kind", { kind });

export const listMeetings = (limit = 100) => invoke<Meeting[]>("list_meetings", { limit });
export const searchMeetings = (query: string) =>
  invoke<Meeting[]>("search_meetings", { query });
export const getMeeting = (id: string) => invoke<Meeting | null>("get_meeting", { id });
export const getMeetingDetail = (id: string) =>
  invoke<MeetingDetail | null>("get_meeting_detail", { id });
export const getSegments = (meetingId: string) =>
  invoke<TranscriptSegment[]>("get_segments", { meetingId });
export const setPinned = (id: string, pinned: boolean) =>
  invoke<void>("set_pinned", { id, pinned });
export const setNotes = (id: string, notes: string) => invoke<void>("set_notes", { id, notes });
export const setMeetingTitle = (id: string, title: string) =>
  invoke<void>("set_meeting_title", { id, title });
export const setSpeakerName = (meetingId: string, speakerId: number, name: string) =>
  invoke<Record<string, string>>("set_speaker_name", { meetingId, speakerId, name });
export const markAsYou = (meetingId: string, speakerId: number, isYou: boolean) =>
  invoke<number[]>("mark_as_you", { meetingId, speakerId, isYou });
export const deleteMeeting = (id: string) => invoke<void>("delete_meeting", { id });

export const coachingOverview = (meetingId: string) =>
  invoke<TrainingMetrics>("coaching_overview", { meetingId });
export const coachingReport = (limit = 30) =>
  invoke<CoachingOverview>("coaching_report", { limit });

export const insightsFinishing = () => invoke<string[]>("insights_finishing");
export const setSalesEnabled = (meetingId: string, enabled: boolean) =>
  invoke<void>("set_sales_enabled", { meetingId, enabled });
export const regenerateInsights = (meetingId: string) =>
  invoke<void>("regenerate_insights", { meetingId });
export const catchUp = (meetingId: string) => invoke<CatchUp>("catch_up", { meetingId });
export const investigate = (meetingId: string, scope: InvestigationScope, focus: string) =>
  invoke<Investigation>("investigate", { meetingId, scope, focus });
export const lookupDocTopic = (meetingId: string, label: string) =>
  invoke<void>("lookup_doc_topic", { meetingId, label });
export const probeDocsMcp = (url: string) =>
  invoke<{ search_tool: string; tools: string[] }>("probe_docs_mcp", { url });
export const pickFolder = () => invoke<string | null>("pick_folder");
export const meetingMarkdown = (meetingId: string) => invoke<string>("meeting_markdown", { meetingId });
export const exportMarkdown = (meetingId: string) => invoke<string | null>("export_markdown", { meetingId });
export const importGranolaCsv = () =>
  invoke<{ imported: number; duplicates: number; skipped_rows: number } | null>("import_granola_csv");
export const deleteSegment = (meetingId: string, segmentId: string) =>
  invoke<void>("delete_segment", { meetingId, segmentId });
export const trimTranscript = (meetingId: string, beforeS: number | null, afterS: number | null) =>
  invoke<number>("trim_transcript", { meetingId, beforeS, afterS });

export const notify = (title: string, body: string) => invoke<void>("notify", { title, body });
export const showMainWindow = () => invoke<void>("show_main_window");
export const smartDecision = (promptId: string, choice: "primary" | "secondary" | "tertiary") =>
  invoke<void>("smart_decision", { promptId, choice });

export const googleStatus = () => invoke<{ connected: boolean; email: string | null }>("google_status");
export const googleConnect = () => invoke<string>("google_connect");
export const googleDisconnect = () => invoke<void>("google_disconnect");
export const calendarEvents = (refresh = false) => invoke<CalendarView>("calendar_events", { refresh });
export const getPrepNotes = (eventId: string) => invoke<string>("get_prep_notes", { eventId });
export const setPrepNotes = (eventId: string, notes: string) => invoke<void>("set_prep_notes", { eventId, notes });
export const startMeetingFromEvent = (eventId: string) => invoke<string>("start_meeting_from_event", { eventId });
export const crmStatus = (provider: CrmProvider) =>
  invoke<{ connected: boolean; account_label: string | null }>("crm_status", { provider });
export const crmConnect = (provider: CrmProvider) => invoke<string>("crm_connect", { provider });
export const crmSearch = (provider: CrmProvider, query: string) => invoke<CrmRecord[]>("crm_search", { provider, query });
export const crmPreview = (meetingId: string) => invoke<{ payload: unknown; tasks: CrmTask[] }>("crm_preview", { meetingId });
export const crmSend = (provider: CrmProvider, meetingId: string, targetObject: string, targetRecordId: string, tasks: CrmTask[]) =>
  invoke<unknown>("crm_send", { provider, meetingId, targetObject, targetRecordId, tasks });
export function onCalendarUpdated(handler: (payload: unknown) => void): Promise<UnlistenFn> {
  return listen<unknown>("calendar_updated", (e) => handler(e.payload));
}

export function onSmartPrompt(handler: (p: SmartPrompt) => void): Promise<UnlistenFn> {
  return listen<SmartPrompt>("smart_prompt", (e) => handler(e.payload));
}
export function onPresence(handler: (p: RecordingPresence) => void): Promise<UnlistenFn> {
  return listen<RecordingPresence>("presence", (e) => handler(e.payload));
}
export function onPresenceAttention(handler: (payload: unknown) => void): Promise<UnlistenFn> {
  return listen<null>("presence_attention", (e) => handler(e.payload));
}
export function onPresenceSettle(handler: (payload: unknown) => void): Promise<UnlistenFn> {
  return listen<null>("presence_settle", (e) => handler(e.payload));
}
export function onRecordingNudge(handler: (n: RecordingNudge) => void): Promise<UnlistenFn> {
  return listen<RecordingNudge>("recording_nudge", (e) => handler(e.payload));
}
export function onDeepLink(handler: (ev: DeepLinkEvent) => void): Promise<UnlistenFn> {
  return listen<DeepLinkEvent>("deep_link", (e) => handler(e.payload));
}
export function onNavigateMeeting(handler: (id: string) => void): Promise<UnlistenFn> {
  return listen<string>("navigate_meeting", (e) => handler(e.payload));
}

export function onInsightsUpdated(handler: (meetingId: string) => void): Promise<UnlistenFn> {
  return listen<string>("insights_updated", (e) => handler(e.payload));
}
export function onInsightsStatus(handler: (ev: InsightsStatusEvent) => void): Promise<UnlistenFn> {
  return listen<InsightsStatusEvent>("insights_status", (e) => handler(e.payload));
}
export function onInvestigationSuggested(
  handler: (ev: { meeting_id: string; focus: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ meeting_id: string; focus: string }>("investigation_suggested", (e) => handler(e.payload));
}
export function onSalesSuggested(handler: (meetingId: string) => void): Promise<UnlistenFn> {
  return listen<string>("sales_suggested", (e) => handler(e.payload));
}

export function onTranscript(
  handler: (ev: TranscriptEventPayload) => void,
): Promise<UnlistenFn> {
  return listen<TranscriptEventPayload>("transcript", (e) => handler(e.payload));
}

export function onTranscriptionStatus(
  handler: (st: StreamStatus) => void,
): Promise<UnlistenFn> {
  return listen<StreamStatus>("transcription_status", (e) => handler(e.payload));
}

export function onMeetingSaved(handler: (id: string) => void): Promise<UnlistenFn> {
  return listen<string>("meeting_saved", (e) => handler(e.payload));
}
