import { useCallback, useEffect, useState } from "react";
import "./App.css";
import { authStatus, getPrefs, hasBridge, launchGate } from "./api";
import type { LaunchGate } from "./types";
import { StoreProvider } from "./store";
import { Shell } from "./views/Shell";
import { ForceUpdate, Onboarding, Terms } from "./views/Gates";
import { Enroll } from "./views/Enroll";
import { Presence } from "./views/Presence";

const IS_PRESENCE = typeof window !== "undefined" && window.location.hash === "#presence";

/** macOS offers compact / standard / large; standard keeps the original metrics here. */
export function applyInterfaceScale(scale: string | undefined) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.scale = scale === "compact" || scale === "large" ? scale : "standard";
}

function App() {
  if (IS_PRESENCE) return <Presence />;
  return <MainApp />;
}

function MainApp() {
  const [gate, setGate] = useState<LaunchGate | null>(null);
  const [gateChecked, setGateChecked] = useState(!hasBridge);
  // Managed mode needs an enrolled device (recovery key); BYOK never does.
  const [needsEnroll, setNeedsEnroll] = useState(false);

  const refreshGate = useCallback(() => {
    if (!hasBridge) return;
    Promise.all([launchGate(), getPrefs(), authStatus()])
      .then(([g, prefs, auth]) => {
        setGate(g);
        setNeedsEnroll(prefs.app_mode === "managed" && !auth.enrolled);
        applyInterfaceScale(prefs.interface_scale);
      })
      .catch(() => setGate(null))
      .finally(() => setGateChecked(true));
  }, []);

  useEffect(() => {
    refreshGate();
  }, [refreshGate]);

  if (!gateChecked) {
    return (
      <div className="gate">
        <span className="brand-mark">miniti</span>
        <p className="muted">starting…</p>
      </div>
    );
  }

  if (gate?.gate === "force_update") return <ForceUpdate gate={gate} />;
  if (gate?.gate === "terms") return <Terms gate={gate} onDone={refreshGate} />;
  if (gate?.gate === "onboarding") return <Onboarding gate={gate} onDone={refreshGate} />;
  if (needsEnroll) return <Enroll onDone={refreshGate} />;

  return (
    <StoreProvider>
      <Shell gate={gate} />
    </StoreProvider>
  );
}

export default App;
