import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  CoachingOverview,
  EnvHealth,
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
