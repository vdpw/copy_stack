import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";
import "./App.css";
import { DiagnosticErrorBanner } from "./components/DiagnosticErrorBanner";
import { HistoryView } from "./features/history/HistoryView";
import { SettingsView } from "./features/settings/SettingsView";
import { detectSystemLanguage, getMessages, isSupportedLanguage } from "./i18n";
import { useAppSettings } from "./hooks/useAppSettings";
import { useStartupErrors } from "./hooks/useStartupErrors";

const isSettingsWindow = getCurrentWindow().label === "settings";

function App() {
  const startup = useStartupErrors();
  const settingsController = useAppSettings(isSettingsWindow, startup.ready);
  const { loadSettings, reportError } = settingsController;
  const language =
    settingsController.settings &&
    isSupportedLanguage(settingsController.settings.resolved_language)
      ? settingsController.settings.resolved_language
      : detectSystemLanguage();
  const messages = getMessages(language);

  useEffect(() => {
    document.documentElement.lang = language;
    document.title = isSettingsWindow ? messages.settings : "Copy Stack";
  }, [language, messages.settings]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void listen("app-language-changed", () => {
      void loadSettings();
    })
      .then(listener => {
        if (disposed) {
          listener();
        } else {
          unlisten = listener;
        }
      })
      .catch(caught => {
        reportError(caught, "load_settings", () => {
          void loadSettings();
        });
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [loadSettings, reportError]);

  return (
    <div className={`app-shell ${isSettingsWindow ? "settings-shell" : ""}`}>
      {startup.error && (
        <DiagnosticErrorBanner
          error={startup.error}
          messages={messages}
          onDismiss={startup.dismiss}
          onRetry={() => void startup.refresh()}
        />
      )}
      {!startup.ready ? (
        <div className="startup-loading" role="status">
          {messages.starting}
        </div>
      ) : isSettingsWindow ? (
        <SettingsView
          controller={settingsController}
          language={language}
          messages={messages}
        />
      ) : (
        <>
          {settingsController.error && (
            <DiagnosticErrorBanner
              error={settingsController.error}
              messages={messages}
              onDismiss={settingsController.dismissError}
              onRetry={settingsController.retryError}
            />
          )}
          <HistoryView
            compactMode={settingsController.settings?.compact_mode ?? false}
            language={language}
            messages={messages}
            onHistoryChanged={settingsController.loadSettings}
          />
        </>
      )}
    </div>
  );
}

export default App;
