import {
  AlertTriangle,
  ArrowLeft,
  ArrowUpDown,
  Eye,
  EyeOff,
  Trash2,
  Type,
} from "lucide-react";
import { useEffect, useState } from "react";
import { invokeCommand } from "../../api/tauri";
import { DiagnosticErrorBanner } from "../../components/DiagnosticErrorBanner";
import {
  isLanguagePreference,
  languageDisplayNames,
  languagePreferences,
} from "../../i18n";
import type { Messages, SupportedLanguage } from "../../i18n";
import type { AppSettingsController } from "../../hooks/useAppSettings";
import { formatBytes } from "../../lib/display";

interface SettingsViewProps {
  controller: AppSettingsController;
  language: SupportedLanguage;
  messages: Messages;
  onBack: () => void;
}

const mebibyte = 1024 * 1024;

export function SettingsView({
  controller,
  language,
  messages,
  onBack,
}: SettingsViewProps) {
  const { settings } = controller;
  const [pendingMaxItemsInput, setPendingMaxItemsInput] = useState("100");
  const [pendingHistoryBudgetInput, setPendingHistoryBudgetInput] =
    useState("256");
  const [pendingMenuBarItemLimitInput, setPendingMenuBarItemLimitInput] =
    useState("0");
  const [showConfirmDialog, setShowConfirmDialog] = useState(false);
  const [clearingHistory, setClearingHistory] = useState(false);

  const settingsHeader = (
    <header className="preferences-header">
      <button
        aria-label={messages.backToHistory}
        className="settings-back-button"
        onClick={onBack}
        title={messages.backToHistory}
        type="button"
      >
        <ArrowLeft aria-hidden="true" size={18} />
      </button>
      <h1>{messages.settings}</h1>
    </header>
  );

  useEffect(() => {
    if (settings) {
      setPendingMaxItemsInput(String(settings.max_items));
      setPendingHistoryBudgetInput(
        String(Math.round(settings.max_history_bytes / mebibyte))
      );
      setPendingMenuBarItemLimitInput(String(settings.menu_bar_item_limit));
    }
  }, [settings]);

  if (!settings) {
    return (
      <main className="content-panel preferences-panel settings-panel">
        {settingsHeader}
        {controller.error ? (
          <DiagnosticErrorBanner
            error={controller.error}
            messages={messages}
            onDismiss={controller.dismissError}
            onRetry={controller.retryError}
          />
        ) : (
          <div className="settings-loading" role="status">
            {messages.loadingHistory}
          </div>
        )}
      </main>
    );
  }

  const parsedPendingMaxItems = Number.parseInt(pendingMaxItemsInput, 10);
  const isPendingMaxItemsValid =
    Number.isInteger(parsedPendingMaxItems) &&
    parsedPendingMaxItems >= 1 &&
    parsedPendingMaxItems <= 1000;
  const isStorageLimitDirty =
    isPendingMaxItemsValid && parsedPendingMaxItems !== settings.max_items;
  const parsedHistoryBudget = Number.parseInt(pendingHistoryBudgetInput, 10);
  const isHistoryBudgetValid =
    Number.isInteger(parsedHistoryBudget) &&
    parsedHistoryBudget >= 16 &&
    parsedHistoryBudget <= 4096;
  const isHistoryBudgetDirty =
    isHistoryBudgetValid &&
    parsedHistoryBudget * mebibyte !== settings.max_history_bytes;
  const parsedMenuBarItemLimit = Number(pendingMenuBarItemLimitInput);
  const isMenuBarItemLimitValid =
    pendingMenuBarItemLimitInput.trim() !== "" &&
    Number.isInteger(parsedMenuBarItemLimit) &&
    parsedMenuBarItemLimit >= 0 &&
    parsedMenuBarItemLimit <= 1000;
  const isMenuBarItemLimitDirty =
    isMenuBarItemLimitValid &&
    parsedMenuBarItemLimit !== settings.menu_bar_item_limit;
  const eventsToDelete = Math.max(
    0,
    settings.history_count - parsedPendingMaxItems
  );

  const applyStorageLimit = async () => {
    if (!isPendingMaxItemsValid || !isStorageLimitDirty) {
      return;
    }
    if (eventsToDelete > 0) {
      setShowConfirmDialog(true);
      return;
    }
    await controller.updateMaxItems(parsedPendingMaxItems);
  };

  const confirmStorageLimit = async () => {
    setShowConfirmDialog(false);
    if (isPendingMaxItemsValid) {
      await controller.updateMaxItems(parsedPendingMaxItems);
    }
  };

  const clearAllEvents = async () => {
    if (
      clearingHistory ||
      controller.updating ||
      settings.history_count === 0
    ) {
      return;
    }

    setClearingHistory(true);
    controller.dismissError();
    try {
      await invokeCommand<void>("clear_all_events", "clear_history");
      await controller.loadSettings();
    } catch (caught) {
      controller.reportError(caught, "clear_history", () => {
        void clearAllEvents();
      });
    } finally {
      setClearingHistory(false);
    }
  };

  return (
    <>
      <main className="content-panel preferences-panel settings-panel">
        {settingsHeader}

        {controller.error && (
          <DiagnosticErrorBanner
            error={controller.error}
            messages={messages}
            onDismiss={controller.dismissError}
            onRetry={controller.retryError}
          />
        )}

        <section className="preference-group">
          <div className="preference-row">
            <span className="preference-copy">
              <label htmlFor="language-select">{messages.language}</label>
              <span className="preference-description">
                {settings.language === "system"
                  ? messages.languageDescriptionSystem(
                      languageDisplayNames[language]
                    )
                  : messages.languageDescriptionManual}
              </span>
            </span>
            <select
              className="language-select"
              disabled={controller.updating}
              id="language-select"
              onChange={event => {
                if (isLanguagePreference(event.target.value)) {
                  void controller.updateLanguage(event.target.value);
                }
              }}
              value={settings.language}
            >
              {languagePreferences.map(preference => (
                <option key={preference} value={preference}>
                  {preference === "system"
                    ? messages.systemDefault
                    : languageDisplayNames[preference]}
                </option>
              ))}
            </select>
          </div>

          <div className="preference-row preference-row-stacked">
            <div className="preference-copy">
              <label htmlFor="max-items-input">{messages.storedItems}</label>
              <p>
                {messages.storedItemsDescription(
                  settings.max_items,
                  settings.history_count
                )}
              </p>
              <p>
                {messages.historyStorageUsage(
                  formatBytes(settings.history_bytes, language),
                  formatBytes(settings.history_limit_bytes, language)
                )}
              </p>
              <p>
                {messages.maximumEventSize(
                  formatBytes(settings.max_event_bytes, language)
                )}
              </p>
            </div>
            <div className="preference-control storage-input-row">
              <input
                className="storage-input"
                disabled={controller.updating}
                id="max-items-input"
                max="1000"
                min="1"
                onChange={event => setPendingMaxItemsInput(event.target.value)}
                type="number"
                value={pendingMaxItemsInput}
              />
              <button
                className="btn btn-primary"
                disabled={
                  controller.updating ||
                  !isPendingMaxItemsValid ||
                  !isStorageLimitDirty
                }
                onClick={() => void applyStorageLimit()}
                type="button"
              >
                {messages.apply}
              </button>
            </div>
            {!isPendingMaxItemsValid && (
              <p className="settings-error" role="alert">
                {messages.storageLimitError}
              </p>
            )}
          </div>

          <div className="preference-row preference-row-stacked">
            <div className="preference-copy">
              <label htmlFor="history-budget-input">
                {messages.historyBudget}
              </label>
              <p>{messages.historyBudgetDescription}</p>
            </div>
            <div className="preference-control storage-input-row">
              <input
                className="storage-input history-budget-input"
                disabled={controller.updating}
                id="history-budget-input"
                max="4096"
                min="16"
                onChange={event =>
                  setPendingHistoryBudgetInput(event.target.value)
                }
                type="number"
                value={pendingHistoryBudgetInput}
              />
              <span className="storage-unit">MiB</span>
              <button
                className="btn btn-primary"
                disabled={
                  controller.updating ||
                  !isHistoryBudgetValid ||
                  !isHistoryBudgetDirty
                }
                onClick={() =>
                  void controller.updateMaxHistoryBytes(
                    parsedHistoryBudget * mebibyte
                  )
                }
                type="button"
              >
                {messages.apply}
              </button>
            </div>
            {!isHistoryBudgetValid && (
              <p className="settings-error" role="alert">
                {messages.historyBudgetError}
              </p>
            )}
          </div>

          <label className="preference-row preference-row-stacked">
            <span className="preference-copy">
              <span className="preference-title">{messages.launchAtLogin}</span>
              <span className="preference-description">
                {controller.autostartLoading
                  ? messages.launchAtLoginLoading
                  : controller.autostartEnabled
                    ? messages.launchAtLoginEnabled
                    : messages.launchAtLoginDisabled}
              </span>
            </span>
            <span className="mac-switch">
              <input
                aria-describedby={
                  controller.autostartError
                    ? "autostart-setting-error"
                    : undefined
                }
                checked={controller.autostartEnabled}
                disabled={controller.autostartLoading || controller.updating}
                onChange={event =>
                  void controller.updateAutostart(event.target.checked)
                }
                type="checkbox"
              />
              <span className="mac-switch-track" />
            </span>
            {controller.autostartError && (
              <span
                className="settings-error autostart-error"
                id="autostart-setting-error"
                role="alert"
              >
                {controller.autostartError === "read"
                  ? messages.launchAtLoginReadError
                  : messages.launchAtLoginUpdateError}
              </span>
            )}
          </label>

          <label className="preference-row">
            <span className="preference-copy">
              <span className="preference-title">{messages.compactMode}</span>
              <span className="preference-description">
                <Type size={13} />
                {settings.compact_mode
                  ? messages.compactModeEnabled
                  : messages.compactModeDisabled}
              </span>
            </span>
            <span className="mac-switch">
              <input
                checked={settings.compact_mode}
                disabled={controller.updating}
                onChange={event =>
                  void controller.updateCompactMode(event.target.checked)
                }
                type="checkbox"
              />
              <span className="mac-switch-track" />
            </span>
          </label>

          <label className="preference-row">
            <span className="preference-copy">
              <span className="preference-title">
                {messages.moveRestoredItemsToTop}
              </span>
              <span className="preference-description">
                <ArrowUpDown size={13} />
                {settings.move_restored_item_to_top
                  ? messages.restoreOrderingEnabled
                  : messages.restoreOrderingDisabled}
              </span>
            </span>
            <span className="mac-switch">
              <input
                checked={settings.move_restored_item_to_top}
                disabled={controller.updating}
                onChange={event =>
                  void controller.updateRestoreOrdering(event.target.checked)
                }
                type="checkbox"
              />
              <span className="mac-switch-track" />
            </span>
          </label>

          <label className="preference-row">
            <span className="preference-copy">
              <span className="preference-title">{messages.showInMenuBar}</span>
              <span className="preference-description">
                {settings.show_in_menu_bar ? (
                  <Eye size={13} />
                ) : (
                  <EyeOff size={13} />
                )}
                {settings.show_in_menu_bar
                  ? messages.menuBarEnabled
                  : messages.menuBarDisabled}
              </span>
            </span>
            <span className="mac-switch">
              <input
                checked={settings.show_in_menu_bar}
                disabled={controller.updating}
                onChange={event =>
                  void controller.updateMenuBarVisibility(event.target.checked)
                }
                type="checkbox"
              />
              <span className="mac-switch-track" />
            </span>
          </label>

          <div className="preference-row preference-row-stacked">
            <div className="preference-copy">
              <label htmlFor="menu-bar-item-limit-input">
                {messages.menuBarItemLimit}
              </label>
              <p>
                {messages.menuBarItemLimitDescription(
                  settings.menu_bar_item_limit
                )}
              </p>
            </div>
            <div className="preference-control storage-input-row">
              <input
                className="storage-input"
                disabled={controller.updating}
                id="menu-bar-item-limit-input"
                max="1000"
                min="0"
                onChange={event =>
                  setPendingMenuBarItemLimitInput(event.target.value)
                }
                type="number"
                value={pendingMenuBarItemLimitInput}
              />
              <button
                className="btn btn-primary"
                disabled={
                  controller.updating ||
                  !isMenuBarItemLimitValid ||
                  !isMenuBarItemLimitDirty
                }
                onClick={() =>
                  void controller.updateMenuBarItemLimit(parsedMenuBarItemLimit)
                }
                type="button"
              >
                {messages.apply}
              </button>
            </div>
            {!isMenuBarItemLimitValid && (
              <p className="settings-error" role="alert">
                {messages.menuBarItemLimitError}
              </p>
            )}
          </div>
        </section>

        <section className="preference-group">
          <div className="preference-row">
            <span className="preference-copy">
              <span className="preference-title">
                {messages.clipboardHistory}
              </span>
              <span className="preference-description">
                {messages.clearHistoryDescription(settings.history_count)}
              </span>
            </span>
            <button
              className="btn btn-danger settings-clear-button"
              disabled={
                clearingHistory ||
                controller.updating ||
                settings.history_count === 0
              }
              onClick={() => void clearAllEvents()}
              type="button"
            >
              <Trash2 aria-hidden="true" size={15} />
              {clearingHistory ? messages.clearingHistory : messages.clearAll}
            </button>
          </div>
        </section>
      </main>

      {showConfirmDialog && (
        <div className="modal-overlay">
          <div
            aria-labelledby="reduce-history-title"
            aria-modal="true"
            className="modal-content"
            role="dialog"
          >
            <div className="modal-header">
              <AlertTriangle className="warning-icon" size={24} />
              <h3 id="reduce-history-title">{messages.reduceHistory}</h3>
            </div>

            <div className="modal-body">
              <p>
                {messages.reduceHistoryDescription(
                  settings.max_items,
                  parsedPendingMaxItems,
                  eventsToDelete
                )}
              </p>
              <p className="warning-text">{messages.cannotUndo}</p>
            </div>

            <div className="modal-actions">
              <button
                className="btn btn-secondary"
                disabled={controller.updating}
                onClick={() => {
                  setShowConfirmDialog(false);
                  setPendingMaxItemsInput(String(settings.max_items));
                }}
                type="button"
              >
                {messages.cancel}
              </button>
              <button
                className="btn btn-danger"
                disabled={controller.updating}
                onClick={() => void confirmStorageLimit()}
                type="button"
              >
                {controller.updating
                  ? messages.updating
                  : messages.deleteAndUpdate}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
