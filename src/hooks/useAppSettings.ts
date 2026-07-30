import { useCallback, useEffect, useRef, useState } from "react";
import {
  invokeCommand,
  normalizeCommandError,
  TauriCommandError,
} from "../api/tauri";
import type { AppSettings, Operation } from "../types";
import type { LanguagePreference } from "../i18n";
import { runOptimisticMutation } from "./settingsMutation";

type AutostartError = "read" | "update" | null;

interface MutationSpec {
  command: string;
  args: Record<string, unknown>;
  patch: Partial<AppSettings>;
}

export function useAppSettings(loadAutostart: boolean, enabled = true) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [updating, setUpdating] = useState(false);
  const [error, setError] = useState<TauriCommandError | null>(null);
  const retryRef = useRef<(() => void) | null>(null);
  const mountedRef = useRef(true);

  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [autostartLoading, setAutostartLoading] = useState(loadAutostart);
  const [autostartError, setAutostartError] = useState<AutostartError>(null);

  const reportError = useCallback(
    (
      caught: unknown,
      operation: Operation,
      retry: (() => void) | null = null
    ) => {
      if (!mountedRef.current) {
        return;
      }
      retryRef.current = retry;
      setError(normalizeCommandError(caught, operation));
    },
    []
  );

  const loadSettings = useCallback(async (): Promise<AppSettings | null> => {
    setLoading(true);
    try {
      const loaded = await invokeCommand<AppSettings>(
        "get_app_settings",
        "load_settings"
      );
      if (mountedRef.current) {
        setSettings(loaded);
        setError(null);
        retryRef.current = null;
      }
      return loaded;
    } catch (caught) {
      reportError(caught, "load_settings", () => {
        void loadSettings();
      });
      return null;
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, [reportError]);

  const reloadAfterFailure = useCallback(async () => {
    try {
      const loaded = await invokeCommand<AppSettings>(
        "get_app_settings",
        "load_settings"
      );
      if (mountedRef.current) {
        setSettings(loaded);
      }
    } catch (caught) {
      reportError(caught, "load_settings", () => {
        void loadSettings();
      });
    }
  }, [loadSettings, reportError]);

  const runSettingsMutation = useCallback(
    async (spec: MutationSpec, retry: () => void) => {
      if (!settings || updating) {
        return;
      }

      const previous = settings;
      setUpdating(true);
      setError(null);
      try {
        await runOptimisticMutation({
          previous,
          optimistic: { ...previous, ...spec.patch },
          apply: setSettings,
          mutate: async () => {
            await invokeCommand<void>(
              spec.command,
              "update_settings",
              spec.args
            );
            return await invokeCommand<AppSettings>(
              "get_app_settings",
              "load_settings"
            );
          },
        });
      } catch (caught) {
        reportError(caught, "update_settings", retry);
        await reloadAfterFailure();
      } finally {
        if (mountedRef.current) {
          setUpdating(false);
        }
      }
    },
    [reloadAfterFailure, reportError, settings, updating]
  );

  const updateMaxItems = useCallback(
    async (maxItems: number) => {
      await runSettingsMutation(
        {
          command: "set_max_items",
          args: { maxItems },
          patch: { max_items: maxItems },
        },
        () => {
          void updateMaxItems(maxItems);
        }
      );
    },
    [runSettingsMutation]
  );

  const updateMaxHistoryBytes = useCallback(
    async (maxHistoryBytes: number) => {
      await runSettingsMutation(
        {
          command: "set_max_history_bytes",
          args: { maxHistoryBytes },
          patch: {
            max_history_bytes: maxHistoryBytes,
            history_limit_bytes: maxHistoryBytes,
          },
        },
        () => {
          void updateMaxHistoryBytes(maxHistoryBytes);
        }
      );
    },
    [runSettingsMutation]
  );

  const updateMenuBarVisibility = useCallback(
    async (showInMenuBar: boolean) => {
      await runSettingsMutation(
        {
          command: "set_show_in_menu_bar",
          args: { showInMenuBar },
          patch: { show_in_menu_bar: showInMenuBar },
        },
        () => {
          void updateMenuBarVisibility(showInMenuBar);
        }
      );
    },
    [runSettingsMutation]
  );

  const updateMenuBarItemLimit = useCallback(
    async (menuBarItemLimit: number) => {
      await runSettingsMutation(
        {
          command: "set_menu_bar_item_limit",
          args: { menuBarItemLimit },
          patch: { menu_bar_item_limit: menuBarItemLimit },
        },
        () => {
          void updateMenuBarItemLimit(menuBarItemLimit);
        }
      );
    },
    [runSettingsMutation]
  );

  const updateRestoreOrdering = useCallback(
    async (moveRestoredItemToTop: boolean) => {
      await runSettingsMutation(
        {
          command: "set_move_restored_item_to_top",
          args: { moveRestoredItemToTop },
          patch: { move_restored_item_to_top: moveRestoredItemToTop },
        },
        () => {
          void updateRestoreOrdering(moveRestoredItemToTop);
        }
      );
    },
    [runSettingsMutation]
  );

  const updateCompactMode = useCallback(
    async (compactMode: boolean) => {
      await runSettingsMutation(
        {
          command: "set_compact_mode",
          args: { compactMode },
          patch: { compact_mode: compactMode },
        },
        () => {
          void updateCompactMode(compactMode);
        }
      );
    },
    [runSettingsMutation]
  );

  const updateLanguage = useCallback(
    async (language: LanguagePreference) => {
      if (!settings || updating) {
        return;
      }

      const previous = settings;
      setUpdating(true);
      setError(null);
      try {
        await runOptimisticMutation({
          previous,
          optimistic: { ...previous, language },
          apply: setSettings,
          mutate: () =>
            invokeCommand<AppSettings>("set_language", "update_settings", {
              language,
            }),
        });
      } catch (caught) {
        reportError(caught, "update_settings", () => {
          void updateLanguage(language);
        });
        await reloadAfterFailure();
      } finally {
        if (mountedRef.current) {
          setUpdating(false);
        }
      }
    },
    [reloadAfterFailure, reportError, settings, updating]
  );

  const loadAutostartStatus = useCallback(async () => {
    setAutostartLoading(true);
    setAutostartError(null);
    try {
      const enabled = await invokeCommand<boolean>(
        "get_autostart_status",
        "load_settings"
      );
      if (mountedRef.current) {
        setAutostartEnabled(enabled);
      }
    } catch (caught) {
      if (mountedRef.current) {
        setAutostartError("read");
      }
      reportError(caught, "update_autostart", () => {
        void loadAutostartStatus();
      });
    } finally {
      if (mountedRef.current) {
        setAutostartLoading(false);
      }
    }
  }, [reportError]);

  const updateAutostart = useCallback(
    async (enabled: boolean) => {
      const previous = autostartEnabled;
      setAutostartEnabled(enabled);
      setAutostartError(null);
      setAutostartLoading(true);

      try {
        const actualEnabled = await invokeCommand<boolean>(
          "set_autostart_enabled",
          "update_autostart",
          { enabled }
        );
        if (mountedRef.current) {
          setAutostartEnabled(actualEnabled);
          if (actualEnabled !== enabled) {
            setAutostartError("update");
            reportError(
              new TauriCommandError({
                code: "autostart_verification_failed",
                operation: "update_autostart",
                retryable: true,
              }),
              "update_autostart",
              () => {
                void updateAutostart(enabled);
              }
            );
          }
        }
      } catch (caught) {
        reportError(caught, "update_autostart", () => {
          void updateAutostart(enabled);
        });
        try {
          const actualEnabled = await invokeCommand<boolean>(
            "get_autostart_status",
            "load_settings"
          );
          if (mountedRef.current) {
            setAutostartEnabled(actualEnabled);
          }
        } catch (reloadError) {
          if (mountedRef.current) {
            setAutostartEnabled(previous);
          }
          reportError(reloadError, "update_autostart", () => {
            void loadAutostartStatus();
          });
        }
        if (mountedRef.current) {
          setAutostartError("update");
        }
      } finally {
        if (mountedRef.current) {
          setAutostartLoading(false);
        }
      }
    },
    [autostartEnabled, loadAutostartStatus, reportError]
  );

  const retryError = useCallback(() => {
    retryRef.current?.();
  }, []);

  const dismissError = useCallback(() => {
    retryRef.current = null;
    setError(null);
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    if (!enabled) {
      return () => {
        mountedRef.current = false;
      };
    }
    void loadSettings();
    if (loadAutostart) {
      void loadAutostartStatus();
    }
    return () => {
      mountedRef.current = false;
    };
  }, [enabled, loadAutostart, loadAutostartStatus, loadSettings]);

  return {
    settings,
    loading,
    updating,
    error,
    autostartEnabled,
    autostartLoading,
    autostartError,
    loadSettings,
    updateMaxItems,
    updateMaxHistoryBytes,
    updateMenuBarVisibility,
    updateMenuBarItemLimit,
    updateRestoreOrdering,
    updateCompactMode,
    updateLanguage,
    updateAutostart,
    reportError,
    retryError,
    dismissError,
  };
}

export type AppSettingsController = ReturnType<typeof useAppSettings>;
