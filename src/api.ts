import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  CoachingMetrics,
  EnvHealth,
  Levels,
  Meeting,
  Prefs,
  TranscriptEventPayload,
  TranscriptSegment,
} from "./types";

/** True when running inside the Tauri shell (vs a plain browser preview). */
export const hasBridge =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function environmentHealth(): Promise<EnvHealth> {
  return invoke<EnvHealth>("environment_health");
}

export function getPrefs(): Promise<Prefs> {
  return invoke<Prefs>("get_prefs");
}

export function setPrefs(prefs: Prefs): Promise<void> {
  return invoke("set_prefs", { prefs });
}

export function getDeviceId(): Promise<string> {
  return invoke<string>("get_device_id");
}

export function startRecording(title: string): Promise<string> {
  return invoke<string>("start_recording", { title });
}

export function stopRecording(): Promise<string | null> {
  return invoke<string | null>("stop_recording");
}

export function getLevels(): Promise<Levels> {
  return invoke<Levels>("get_levels");
}

export function listMeetings(limit = 100): Promise<Meeting[]> {
  return invoke<Meeting[]>("list_meetings", { limit });
}

export function searchMeetings(query: string): Promise<Meeting[]> {
  return invoke<Meeting[]>("search_meetings", { query });
}

export function getMeeting(id: string): Promise<Meeting | null> {
  return invoke<Meeting | null>("get_meeting", { id });
}

export function getSegments(meetingId: string): Promise<TranscriptSegment[]> {
  return invoke<TranscriptSegment[]>("get_segments", { meetingId });
}

export function setPinned(id: string, pinned: boolean): Promise<void> {
  return invoke("set_pinned", { id, pinned });
}

export function deleteMeeting(id: string): Promise<void> {
  return invoke("delete_meeting", { id });
}

export function coachingOverview(meetingId: string): Promise<CoachingMetrics> {
  return invoke<CoachingMetrics>("coaching_overview", { meetingId });
}

export function onTranscript(
  handler: (ev: TranscriptEventPayload) => void,
): Promise<UnlistenFn> {
  return listen<TranscriptEventPayload>("transcript", (e) => handler(e.payload));
}
