import { useCallback, useEffect, useState } from "react";
import "./App.css";
import { authStatus, getPrefs, hasBridge, launchGate, onAuthChanged } from "./api";
import type { LaunchGate } from "./types";
import { StoreProvider } from "./store";
import { Shell } from "./views/Shell";
import { ForceUpdate, Onboarding, Terms } from "./views/Gates";
import { Enroll } from "./views/Enroll";
import { useTauriEvent } from "./useEvent";
import { applyInterfaceScale } from "./scale";
import { Presence } from "./views/Presence";

const IS_PRESENCE = typeof window !== "undefined" && window.location.hash === "#presence";



function App() {
  if (IS_PRESENCE) return <Presence />;
  return <MainApp />;
}

function MainApp() {
  const [gate, setGate] = useState<LaunchGate | null>(null);
  const [gateChecked, setGateChecked] = useState(!hasBridge);
  // Managed mode needs an enrolled device (recovery key); BYOK never does.
  const [needsEnroll, setNeedsEnroll] = useState(false);
  const [keyringPending, setKeyringPending] = useState(false);

  const refreshGate = useCallback(() => {
    if (!hasBridge) return;
    Promise.all([launchGate(), getPrefs(), authStatus()])
      .then(([g, prefs, auth]) => {
        setGate(g);
        setNeedsEnroll(prefs.app_mode === "managed" && !auth.enrolled);
        setKeyringPending(auth.keyring_pending);
        applyInterfaceScale(prefs.interface_scale);
      })
      .catch(() => setGate(null))
      .finally(() => setGateChecked(true));
  }, []);

  useEffect(() => {
    refreshGate();
  }, [refreshGate]);
  useTauriEvent<null>(onAuthChanged, () => refreshGate());

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
  if (needsEnroll) return <Enroll onDone={refreshGate} keyringPending={keyringPending} />;

  return (
    <StoreProvider>
      <Shell gate={gate} />
    </StoreProvider>
  );
}

export default App;
