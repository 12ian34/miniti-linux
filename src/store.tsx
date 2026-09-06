import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  errorMessage,
  getPrefs,
  hasBridge,
  listMeetings,
  onMeetingSaved,
  recordingStatus,
  startRecording,
  stopRecording,
} from "./api";
import { useTauriEvent } from "./useEvent";
import type { Meeting, Prefs, RecordingStatus } from "./types";

export type Route =
  | { kind: "home" }
  | { kind: "meeting"; id: string }
  | { kind: "coaching" }
  | { kind: "settings" };

interface Store {
  route: Route;
  navigate: (r: Route) => void;
  back: () => void;
  canGoBack: boolean;
  sidebarOpen: boolean;
  setSidebarOpen: (v: boolean) => void;
  insightsOpen: boolean;
  setInsightsOpen: (v: boolean) => void;
  recording: RecordingStatus;
  meetings: Meeting[];
  refreshMeetings: () => Promise<void>;
  prefs: Prefs | null;
  refreshPrefs: () => Promise<void>;
  start: (title?: string) => Promise<string>;
  stop: () => Promise<string | null>;
  starting: boolean;
  stopping: boolean;
  lastError: string | null;
  clearError: () => void;
}

const IDLE: RecordingStatus = { recording: false, meeting_id: null, elapsed_seconds: 0, stream: null };

const StoreContext = createContext<Store | null>(null);

function readBool(key: string, fallback: boolean): boolean {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : v === "1";
  } catch {
    return fallback;
  }
}

function writeBool(key: string, v: boolean) {
  try {
    localStorage.setItem(key, v ? "1" : "0");
  } catch {
    /* ignore */
  }
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [route, setRoute] = useState<Route>({ kind: "home" });
  const history = useRef<Route[]>([]);
  const [sidebarOpen, setSidebarOpenState] = useState(() => readBool("ui.sidebar", true));
  const [insightsOpen, setInsightsOpenState] = useState(() => readBool("ui.insights", true));
  const [recording, setRecording] = useState<RecordingStatus>(IDLE);
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [prefs, setPrefs] = useState<Prefs | null>(null);
  const [starting, setStarting] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [lastError, setLastError] = useState<string | null>(null);

  const navigate = useCallback((r: Route) => {
    setRoute((prev) => {
      history.current.push(prev);
      if (history.current.length > 50) history.current.shift();
      return r;
    });
  }, []);

  const back = useCallback(() => {
    const prev = history.current.pop();
    if (prev) setRoute(prev);
    else setRoute({ kind: "home" });
  }, []);

  const setSidebarOpen = useCallback((v: boolean) => {
    setSidebarOpenState(v);
    writeBool("ui.sidebar", v);
  }, []);
  const setInsightsOpen = useCallback((v: boolean) => {
    setInsightsOpenState(v);
    writeBool("ui.insights", v);
  }, []);

  const refreshMeetings = useCallback(async () => {
    if (!hasBridge) return;
    try {
      setMeetings(await listMeetings(500));
    } catch (e) {
      setLastError(errorMessage(e));
    }
  }, []);

  const refreshPrefs = useCallback(async () => {
    if (!hasBridge) return;
    try {
      setPrefs(await getPrefs());
    } catch (e) {
      setLastError(errorMessage(e));
    }
  }, []);

  useEffect(() => {
    void refreshMeetings();
    void refreshPrefs();
  }, [refreshMeetings, refreshPrefs]);

  useTauriEvent(onMeetingSaved, () => void refreshMeetings());

  useEffect(() => {
    if (!hasBridge) return;
    let alive = true;
    const tick = () =>
      recordingStatus()
        .then((s) => alive && setRecording(s))
        .catch(() => {});
    tick();
    const id = window.setInterval(tick, 500);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);

  const start = useCallback(
    async (title?: string) => {
      setStarting(true);
      setLastError(null);
      try {
        const id = await startRecording(title ?? "New meeting");
        setSidebarOpen(false);
        await refreshMeetings();
        navigate({ kind: "meeting", id });
        setRecording(await recordingStatus());
        return id;
      } catch (e) {
        setLastError(errorMessage(e));
        throw e;
      } finally {
        setStarting(false);
      }
    },
    [navigate, refreshMeetings, setSidebarOpen],
  );

  const stop = useCallback(async () => {
    setStopping(true);
    setLastError(null);
    try {
      const id = await stopRecording();
      setRecording(await recordingStatus());
      await refreshMeetings();
      return id;
    } catch (e) {
      setLastError(errorMessage(e));
      throw e;
    } finally {
      setStopping(false);
    }
  }, [refreshMeetings]);

  const value = useMemo<Store>(
    () => ({
      route,
      navigate,
      back,
      canGoBack: history.current.length > 0,
      sidebarOpen,
      setSidebarOpen,
      insightsOpen,
      setInsightsOpen,
      recording,
      meetings,
      refreshMeetings,
      prefs,
      refreshPrefs,
      start,
      stop,
      starting,
      stopping,
      lastError,
      clearError: () => setLastError(null),
    }),
    [
      route, navigate, back, sidebarOpen, setSidebarOpen, insightsOpen, setInsightsOpen,
      recording, meetings, refreshMeetings, prefs, refreshPrefs, start, stop, starting,
      stopping, lastError,
    ],
  );

  return <StoreContext.Provider value={value}>{children}</StoreContext.Provider>;
}

export function useStore(): Store {
  const s = useContext(StoreContext);
  if (!s) throw new Error("useStore outside StoreProvider");
  return s;
}
