import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { invokeCommand, normalizeCommandError } from "../api/tauri";
import type { CommandError } from "../types";

export function useStartupErrors() {
  const [error, setError] = useState<CommandError | null>(null);
  const [ready, setReady] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const startupError = await invokeCommand<CommandError | null>(
        "get_startup_error",
        "startup"
      );
      if (startupError) {
        setError(startupError);
      }
    } catch (caught) {
      setError(normalizeCommandError(caught, "startup"));
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    const bootstrap = async () => {
      try {
        unlisten = await listen<CommandError>("app-operation-error", event => {
          setError(normalizeCommandError(event.payload, "startup"));
        });
        if (disposed) {
          unlisten();
          unlisten = null;
          return;
        }
      } catch (caught) {
        setError(normalizeCommandError(caught, "startup"));
      }

      await refresh();
      if (!disposed) {
        setReady(true);
      }
    };

    void bootstrap();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refresh]);

  return {
    error,
    ready,
    refresh,
    dismiss: () => setError(null),
  };
}
