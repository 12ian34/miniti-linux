import { useEffect } from "react";
import { onNavigateMeeting } from "../api";
import { useTauriEvent } from "../useEvent";
import { useStore } from "../store";
import type { LaunchGate } from "../types";
import { Sidebar } from "./Sidebar";
import { Home } from "./Home";
import { MeetingView } from "./Meeting";
import { Coaching } from "./Coaching";
import { Settings } from "./Settings";
import { SmartPromptBanner } from "./SmartPromptBanner";

/**
 * macOS layout: history sidebar (collapsible, auto-collapses when recording
 * starts) · content · insights rail (owned by the meeting view).
 * Shortcuts: Ctrl+[ sidebar, Ctrl+] insights, Ctrl+, settings, Ctrl+R record.
 */
export function Shell({ gate }: { gate: LaunchGate | null }) {
  const store = useStore();
  const { route, navigate, sidebarOpen, setSidebarOpen, insightsOpen, setInsightsOpen } = store;
  useTauriEvent(onNavigateMeeting, (id) => {
    setSidebarOpen(false);
    void store.refreshMeetings();
    navigate({ kind: "meeting", id });
  });

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const mod = e.ctrlKey || e.metaKey;
      if (!mod) return;
      const target = e.target as HTMLElement | null;
      const editing = target && (target.tagName === "TEXTAREA" || target.tagName === "INPUT");
      if (e.key === "[") {
        e.preventDefault();
        setSidebarOpen(!sidebarOpen);
      } else if (e.key === "]") {
        e.preventDefault();
        setInsightsOpen(!insightsOpen);
      } else if (e.key === ",") {
        e.preventDefault();
        navigate({ kind: "settings" });
      } else if (e.key.toLowerCase() === "r" && !editing && !e.shiftKey) {
        e.preventDefault();
        if (store.recording.recording) void store.stop();
        else void store.start().catch(() => {});
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [sidebarOpen, insightsOpen, setSidebarOpen, setInsightsOpen, navigate, store]);

  let content;
  switch (route.kind) {
    case "home":
      content = <Home gate={gate} />;
      break;
    case "meeting":
      content = <MeetingView id={route.id} key={route.id} />;
      break;
    case "coaching":
      content = <Coaching />;
      break;
    case "settings":
      content = <Settings />;
      break;
  }

  return (
    <div className={`shell ${sidebarOpen ? "" : "sidebar-collapsed"}`}>
      <Sidebar />
      <main className="content">
        <SmartPromptBanner />
        {content}
      </main>
    </div>
  );
}
