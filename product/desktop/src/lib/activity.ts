import { useEffect, useRef, useState } from "react";

type PollingLoopOptions = {
  enabled: boolean;
  intervalMs: number;
  immediate?: boolean;
  initialDelayMs?: number;
};

type DesktopActivity = {
  documentVisible: boolean;
  windowFocused: boolean;
  active: boolean;
};

function readDesktopActivity(): DesktopActivity {
  if (typeof document === "undefined") {
    return {
      documentVisible: true,
      windowFocused: true,
      active: true,
    };
  }
  const documentVisible = !document.hidden;
  const windowFocused = typeof document.hasFocus === "function" ? document.hasFocus() : true;
  return {
    documentVisible,
    windowFocused,
    active: documentVisible && windowFocused,
  };
}

export function useDesktopActivity(): DesktopActivity {
  const [activity, setActivity] = useState<DesktopActivity>(() => readDesktopActivity());

  useEffect(() => {
    const update = () => setActivity(readDesktopActivity());
    document.addEventListener("visibilitychange", update);
    window.addEventListener("focus", update);
    window.addEventListener("blur", update);
    return () => {
      document.removeEventListener("visibilitychange", update);
      window.removeEventListener("focus", update);
      window.removeEventListener("blur", update);
    };
  }, []);

  return activity;
}

export function usePageActivity(pageVisible = true): boolean {
  const activity = useDesktopActivity();
  return pageVisible && activity.active;
}

export function usePollingLoop(
  callback: () => void | Promise<void>,
  { enabled, intervalMs, immediate = true, initialDelayMs }: PollingLoopOptions,
): void {
  const callbackRef = useRef(callback);

  useEffect(() => {
    callbackRef.current = callback;
  }, [callback]);

  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    let timeoutId: number | null = null;

    const schedule = (delayMs: number) => {
      timeoutId = window.setTimeout(async () => {
        await Promise.resolve(callbackRef.current());
        if (cancelled) return;
        schedule(intervalMs);
      }, delayMs);
    };

    schedule(initialDelayMs ?? (immediate ? 0 : intervalMs));
    return () => {
      cancelled = true;
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [enabled, immediate, initialDelayMs, intervalMs]);
}
