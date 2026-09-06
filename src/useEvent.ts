import { useEffect, useRef } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { hasBridge } from "./api";

/**
 * Subscribe to a Tauri event for the lifetime of a component.
 *
 * `listen` resolves asynchronously, so a naive effect that stores the unlisten
 * function in a closure leaks the subscription when React unmounts before the
 * promise settles — which React StrictMode does on every mount in dev. That
 * produced one listener per mount and doubled every transcript line. Here the
 * cleanup runs whether the promise has settled or not.
 */
export function useTauriEvent<T>(
  subscribe: (handler: (payload: T) => void) => Promise<UnlistenFn>,
  handler: (payload: T) => void,
) {
  const latest = useRef(handler);
  latest.current = handler;

  useEffect(() => {
    if (!hasBridge) return;
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;
    subscribe((p) => latest.current(p)).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [subscribe]);
}
