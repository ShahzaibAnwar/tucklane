import { useCallback, useEffect, useRef, useState } from "react";
import { errorMessage, on } from "@/lib/api";
import { useApp } from "@/stores/app";

/**
 * Loads data from the backend and reloads it whenever the backend reports a
 * change (or `deps` change). Never shows fabricated placeholder data.
 */
export function useData<T>(load: () => Promise<T>, deps: unknown[] = []) {
  const tick = useApp((s) => s.tick);
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const loadRef = useRef(load);
  loadRef.current = load;

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      setData(await loadRef.current());
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tick, reload, ...deps]);

  return { data, error, loading, reload, setData };
}

/** Subscribes to a backend event for the lifetime of the component. */
export function useBackendEvent<P>(event: string, cb: (p: P) => void) {
  const ref = useRef(cb);
  ref.current = cb;
  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    void on<P>(event, (p) => ref.current(p)).then((u) => {
      if (cancelled) u();
      else off = u;
    });
    return () => {
      cancelled = true;
      off?.();
    };
  }, [event]);
}
