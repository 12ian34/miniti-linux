import { useState } from "react";
import { onSmartPrompt, smartDecision } from "../api";
import { useTauriEvent } from "../useEvent";
import type { SmartPrompt } from "../types";

/** In-app copy of the Smart-meeting decision (the floating surface and tray carry it too). */
export function SmartPromptBanner() {
  const [prompt, setPrompt] = useState<SmartPrompt | null>(null);
  useTauriEvent(onSmartPrompt, (p) => setPrompt(p.kind === "clear" ? null : p));
  if (!prompt) return null;
  const tone = prompt.kind === "start" ? "ok" : "warn";
  return (
    <div className={`banner ${tone} smart-banner`}>
      <span className="ellipsis">{prompt.message}</span>
      <button className="btn primary small" onClick={() => smartDecision(prompt.id, "primary")}>{prompt.primary}</button>
      <button className="btn secondary small" onClick={() => smartDecision(prompt.id, "secondary")}>{prompt.secondary}</button>
      {prompt.tertiary && (
        <button className="ghost" onClick={() => smartDecision(prompt.id, "tertiary")}>{prompt.tertiary}</button>
      )}
    </div>
  );
}
