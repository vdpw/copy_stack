import { listen } from "@tauri-apps/api/event";
import { Clock3, Settings } from "lucide-react";
import { useEffect, useState } from "react";
import "./App.css";
import { DiagnosticErrorBanner } from "./components/DiagnosticErrorBanner";
import { HistoryView } from "./features/history/HistoryView";
import { SettingsView } from "./features/settings/SettingsView";
import { detectSystemLanguage, getMessages, isSupportedLanguage } from "./i18n";
import { useAppSettings } from "./hooks/useAppSettings";
import { useStartupErrors } from "./hooks/useStartupErrors";
import { isAppPage } from "./navigation";
import type { AppPage } from "./navigation";

function App() {
  const [activePage, setActivePage] = useState<AppPage>("history");
  const startup = useStartupErrors();
  const settingsController = useAppSettings(
    activePage === "settings",
    startup.ready
  );
  const { loadSettings, reportError } = settingsController;
  const language =
    settingsController.settings &&
    isSupportedLanguage(settingsController.settings.resolved_language)
      ? settingsController.settings.resolved_language
      : detectSystemLanguage();
  const messages = getMessages(language);

  useEffect(() => {
    document.documentElement.lang = language;
    document.title =
      activePage === "settings"
        ? `${messages.settings} — Copy Stack`
        : "Copy Stack";
  }, [activePage, language, messages.settings]);

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

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void listen<unknown>("app:navigate", event => {
      if (isAppPage(event.payload)) {
        setActivePage(event.payload);
      }
    })
      .then(listener => {
        if (disposed) {
          listener();
        } else {
          unlisten = listener;
        }
      })
      .catch(caught => {
        reportError(caught, "load_history");
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [reportError]);

  useEffect(() => {
    window.scrollTo({ top: 0 });
  }, [activePage]);

  return (
    <div className="app-shell">
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
      ) : (
        <>
          <header className="app-navigation">
            <div className="app-brand" aria-label="Copy Stack">
              <span className="app-brand-mark" aria-hidden="true">
                C
              </span>
              <span>Copy Stack</span>
            </div>
            <nav className="page-tabs" aria-label="Copy Stack">
              <button
                aria-current={activePage === "history" ? "page" : undefined}
                className={`page-tab ${
                  activePage === "history" ? "page-tab-active" : ""
                }`}
                onClick={() => setActivePage("history")}
                type="button"
              >
                <Clock3 aria-hidden="true" size={15} />
                {messages.clipboardHistory}
              </button>
              <button
                aria-current={activePage === "settings" ? "page" : undefined}
                className={`page-tab ${
                  activePage === "settings" ? "page-tab-active" : ""
                }`}
                onClick={() => setActivePage("settings")}
                type="button"
              >
                <Settings aria-hidden="true" size={15} />
                {messages.settings}
              </button>
            </nav>
          </header>

          {activePage === "settings" ? (
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
        </>
      )}
    </div>
  );
}

export default App;
