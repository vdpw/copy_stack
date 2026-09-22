import {
  AlertTriangle,
  ArrowLeft,
  ArrowUpDown,
  Clipboard,
  Contrast,
  HelpCircle,
  PanelTop,
  Settings,
  Trash2,
  Type,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { ElementRef, KeyboardEvent } from "react";
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
import "./settings.css";

interface SettingsViewProps {
  controller: AppSettingsController;
  language: SupportedLanguage;
  messages: Messages;
  onBack: () => void;
}

const mebibyte = 1024 * 1024;
type SettingsCategory = "general" | "appearance" | "clipboard" | "menu-bar";

function SettingsHelp({
  id,
  label,
  text,
}: {
  id: string;
  label: string;
  text: string;
}) {
  const [open, setOpen] = useState(false);
  const helpRef = useRef<ElementRef<"span">>(null);
  const triggerRef = useRef<ElementRef<"button">>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    const closeOutside = (event: { target: unknown }) => {
      if (
        event.target instanceof window.Node &&
        !helpRef.current?.contains(event.target)
      ) {
        setOpen(false);
      }
    };
    const closeOnEscape = (event: {
      key: string;
      preventDefault: () => void;
    }) => {
      if (event.key === "Escape") {
        event.preventDefault();
        setOpen(false);
        if (helpRef.current?.contains(document.activeElement)) {
          triggerRef.current?.focus();
        }
      }
    };
    document.addEventListener("click", closeOutside);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("click", closeOutside);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  return (
    <span className="settings-help" ref={helpRef}>
      <button
        aria-label={label}
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        aria-describedby={open ? id : undefined}
        className="settings-help-trigger"
        onClick={() => setOpen(previous => !previous)}
        ref={triggerRef}
        type="button"
      >
        <HelpCircle aria-hidden="true" size={15} />
      </button>
      {open && (
        <span className="settings-help-popover" id={id} role="note">
          {text}
        </span>
      )}
    </span>
  );
}

export function SettingsView({
  controller,
  language,
  messages,
  onBack,
}: SettingsViewProps) {
  const { settings } = controller;
  const settingsPageRef = useRef<ElementRef<"div">>(null);
  const settingsShellRef = useRef<ElementRef<"div">>(null);
  const dialogRef = useRef<ElementRef<"div">>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const [activeCategory, setActiveCategory] =
    useState<SettingsCategory>("general");
  const [pendingMaxItemsInput, setPendingMaxItemsInput] = useState("100");
  const [pendingHistoryBudgetInput, setPendingHistoryBudgetInput] =
    useState("256");
  const [pendingEventBudgetInput, setPendingEventBudgetInput] = useState("32");
  const [pendingMenuBarItemLimitInput, setPendingMenuBarItemLimitInput] =
    useState("0");
  const [showConfirmDialog, setShowConfirmDialog] = useState(false);
  const [showClearHistoryDialog, setShowClearHistoryDialog] = useState(false);
  const [clearingHistory, setClearingHistory] = useState(false);
  const dialogOpen = showConfirmDialog || showClearHistoryDialog;
  const savedMaxItems = settings?.max_items;
  const savedHistoryBudget = settings?.max_history_bytes;
  const savedEventBudget = settings?.max_event_bytes;
  const savedMenuBarItemLimit = settings?.menu_bar_item_limit;

  const cancelDialog = () => {
    if (showConfirmDialog && savedMaxItems !== undefined) {
      setPendingMaxItemsInput(String(savedMaxItems));
    }
    setShowConfirmDialog(false);
    setShowClearHistoryDialog(false);
  };

  const handleDialogKeyDown = (event: KeyboardEvent<ElementRef<"div">>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancelDialog();
    } else if (event.key === "Tab") {
      const buttons = Array.from(
        event.currentTarget.querySelectorAll<HTMLElement>(
          "button:not(:disabled)"
        )
      );
      const first = buttons[0];
      const last = buttons[buttons.length - 1];
      if (!first) {
        event.preventDefault();
      } else if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }
  };

  const selectCategory = (category: SettingsCategory) => {
    setActiveCategory(category);
    if (settingsPageRef.current) {
      settingsPageRef.current.scrollTop = 0;
    }
  };

  const categories = [
    {
      id: "general",
      title: messages.generalSettings,
      description: messages.generalSettingsDescription,
      icon: Settings,
    },
    {
      id: "appearance",
      title: messages.appearanceSettings,
      description: messages.appearanceSettingsDescription,
      icon: Contrast,
    },
    {
      id: "clipboard",
      title: messages.clipboardSettings,
      description: messages.clipboardSettingsDescription,
      icon: Clipboard,
    },
    {
      id: "menu-bar",
      title: messages.menuBarSettings,
      description: messages.menuBarSettingsDescription,
      icon: PanelTop,
    },
  ] as const;
  const category =
    categories.find(item => item.id === activeCategory) ?? categories[0];
  const settingsNavigation = (
    <aside className="settings-sidebar">
      <div
        aria-hidden="true"
        className="settings-titlebar-space"
        data-tauri-drag-region="deep"
      />
      <div className="settings-sidebar-scroll">
        <button
          aria-label={messages.backToHistory}
          className="settings-sidebar-back"
          onClick={onBack}
          title={messages.backToHistory}
          type="button"
        >
          <ArrowLeft aria-hidden="true" size={17} />
          <span>{messages.backToHistoryShort}</span>
        </button>
        <div className="settings-sidebar-title">{messages.settings}</div>
        <nav aria-label={messages.settings} className="settings-navigation">
          {categories.map(({ id, title, icon: Icon }) => (
            <button
              aria-current={activeCategory === id ? "page" : undefined}
              key={id}
              onClick={() => selectCategory(id)}
              type="button"
            >
              <span
                aria-hidden="true"
                className={`settings-category-icon settings-category-icon-${id}`}
              >
                <Icon size={17} />
              </span>
              <span>{title}</span>
            </button>
          ))}
        </nav>
      </div>
    </aside>
  );
  const settingsHeader = (
    <header className="settings-page-header" data-tauri-drag-region="deep">
      <h1 id="settings-page-title">{category.title}</h1>
      <p>{category.description}</p>
    </header>
  );

  useEffect(() => {
    if (savedMaxItems !== undefined) {
      setPendingMaxItemsInput(String(savedMaxItems));
    }
  }, [savedMaxItems]);

  useEffect(() => {
    if (savedHistoryBudget !== undefined) {
      setPendingHistoryBudgetInput(
        String(Math.round(savedHistoryBudget / mebibyte))
      );
    }
  }, [savedHistoryBudget]);

  useEffect(() => {
    if (savedEventBudget !== undefined) {
      setPendingEventBudgetInput(String(savedEventBudget / mebibyte));
    }
  }, [savedEventBudget]);

  useEffect(() => {
    if (savedMenuBarItemLimit !== undefined) {
      setPendingMenuBarItemLimitInput(String(savedMenuBarItemLimit));
    }
  }, [savedMenuBarItemLimit]);

  useEffect(() => {
    if (controller.error?.operation === "move_storage") {
      settingsPageRef.current?.scrollTo?.({ top: 0 });
    }
  }, [controller.error]);

  useEffect(() => {
    if (!dialogOpen) {
      return;
    }
    const shell = settingsShellRef.current;
    shell?.setAttribute("inert", "");
    dialogRef.current
      ?.querySelector<HTMLElement>("[data-dialog-cancel]")
      ?.focus();
    return () => shell?.removeAttribute("inert");
  }, [dialogOpen]);

  useEffect(() => {
    if (!dialogOpen && !controller.updating && !clearingHistory) {
      const trigger = returnFocusRef.current;
      returnFocusRef.current = null;
      if (trigger?.isConnected) {
        const target = trigger.matches(":disabled")
          ? (trigger
              .closest(".preference-row")
              ?.querySelector<HTMLElement>("input:not(:disabled)") ??
            settingsPageRef.current)
          : trigger;
        target?.focus();
      }
    }
  }, [dialogOpen, controller.updating, clearingHistory]);

  if (!settings) {
    return (
      <div className="settings-shell settings-panel" ref={settingsShellRef}>
        {settingsNavigation}
        <main aria-labelledby="settings-page-title" className="settings-page">
          {settingsHeader}
          <div
            className="settings-page-scroll"
            ref={settingsPageRef}
            tabIndex={-1}
          >
            {controller.error ? (
              <DiagnosticErrorBanner
                error={controller.error}
                messages={messages}
                onDismiss={controller.dismissError}
                onRetry={controller.retryError}
              />
            ) : (
              <div className="settings-loading" role="status">
                {messages.loadingSettings}
              </div>
            )}
          </div>
        </main>
      </div>
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
  const parsedEventBudget = Number(pendingEventBudgetInput);
  const isEventBudgetValid =
    pendingEventBudgetInput.trim() !== "" &&
    Number.isInteger(parsedEventBudget) &&
    parsedEventBudget >= 1 &&
    parsedEventBudget <= 256;
  const isEventBudgetDirty =
    isEventBudgetValid &&
    parsedEventBudget * mebibyte !== settings.max_event_bytes;
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

  const applyStorageLimit = async (trigger: HTMLElement) => {
    if (!isPendingMaxItemsValid || !isStorageLimitDirty) {
      return;
    }
    if (eventsToDelete > 0) {
      returnFocusRef.current = trigger;
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
      <div className="settings-shell settings-panel" ref={settingsShellRef}>
        {settingsNavigation}
        <main aria-labelledby="settings-page-title" className="settings-page">
          {settingsHeader}
          <div
            className="settings-page-scroll"
            ref={settingsPageRef}
            tabIndex={-1}
          >
            {controller.error && (
              <DiagnosticErrorBanner
                error={controller.error}
                messages={messages}
                onDismiss={controller.dismissError}
                onRetry={controller.retryError}
              />
            )}

            {activeCategory === "general" && (
              <div className="settings-category-content settings-category-general">
                <section className="preference-group">
                  <div className="preference-row preference-row-language">
                    <span className="preference-copy">
                      <label htmlFor="language-select">
                        {messages.language}
                      </label>
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
                  <label className="preference-row preference-row-with-error">
                    <span className="preference-copy">
                      <span className="preference-title">
                        {messages.launchAtLogin}
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
                        disabled={
                          controller.autostartLoading || controller.updating
                        }
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
                </section>
              </div>
            )}
            {activeCategory === "appearance" && (
              <div className="settings-category-content">
                <section className="theme-preference-group">
                  <div className="theme-preference-copy">
                    <h2 id="appearance-theme-title">{messages.theme}</h2>
                  </div>
                  <fieldset
                    aria-labelledby="appearance-theme-title"
                    className="theme-fieldset"
                  >
                    <div className="theme-options">
                      {(["light", "dark", "system"] as const).map(theme => (
                        <label className="theme-option" key={theme}>
                          <input
                            checked={settings.theme === theme}
                            disabled={controller.updating}
                            name="appearance-theme"
                            onChange={() => void controller.updateTheme(theme)}
                            type="radio"
                            value={theme}
                          />
                          <span
                            aria-hidden="true"
                            className={`theme-preview theme-preview-${theme}`}
                          >
                            {(["light", "dark"] as const)
                              .filter(
                                scene => theme === "system" || theme === scene
                              )
                              .map(scene => (
                                <span
                                  className={`theme-preview-scene theme-preview-scene-${scene}`}
                                  key={scene}
                                >
                                  <span className="theme-preview-window-back">
                                    <i />
                                  </span>
                                  <span className="theme-preview-window-front">
                                    <i />
                                    <i />
                                    <i />
                                  </span>
                                </span>
                              ))}
                          </span>
                          <span className="theme-option-label">
                            {theme === "system"
                              ? messages.themeSystem
                              : theme === "light"
                                ? messages.themeLight
                                : messages.themeDark}
                          </span>
                        </label>
                      ))}
                    </div>
                  </fieldset>
                </section>
              </div>
            )}
            {activeCategory === "clipboard" && (
              <div className="settings-category-content">
                <h2 className="preference-group-heading">
                  {messages.behaviorSettings}
                </h2>
                <section className="preference-group">
                  <label className="preference-row">
                    <span className="preference-copy">
                      <span className="preference-title">
                        {messages.compactMode}
                      </span>
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
                          void controller.updateCompactMode(
                            event.target.checked
                          )
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
                          void controller.updateRestoreOrdering(
                            event.target.checked
                          )
                        }
                        type="checkbox"
                      />
                      <span className="mac-switch-track" />
                    </span>
                  </label>
                </section>
                <h2 className="preference-group-heading">
                  {messages.storageSettings}
                </h2>
                <section className="preference-group settings-storage-group">
                  <div
                    aria-busy={controller.movingStorage}
                    className="preference-row settings-storage-location"
                  >
                    <div className="preference-copy">
                      <div className="preference-label-with-help">
                        <span
                          className="preference-title"
                          id="storage-directory-label"
                        >
                          {messages.storageDirectory}
                        </span>
                        <SettingsHelp
                          id="storage-directory-help"
                          label={messages.settingHelp(
                            messages.storageDirectory
                          )}
                          text={messages.storageDirectoryHelp}
                        />
                      </div>
                      <span
                        aria-labelledby="storage-directory-label"
                        className="settings-storage-path"
                        id="storage-directory"
                      >
                        {settings.storage_directory}
                      </span>
                    </div>
                    <button
                      aria-describedby="storage-directory"
                      className="btn btn-secondary"
                      disabled={
                        controller.updating ||
                        controller.autostartLoading ||
                        clearingHistory
                      }
                      onClick={() => void controller.changeStorageDirectory()}
                      type="button"
                    >
                      {messages.changeStorageDirectory}
                    </button>
                    {controller.movingStorage && (
                      <span className="settings-storage-status" role="status">
                        {messages.movingStorage}
                      </span>
                    )}
                  </div>
                  <div className="preference-row preference-row-stacked">
                    <div className="preference-copy">
                      <div className="preference-label-with-help">
                        <label htmlFor="max-items-input">
                          {messages.storedItems}
                        </label>
                        <SettingsHelp
                          id="stored-items-help"
                          label={messages.settingHelp(messages.storedItems)}
                          text={messages.storedItemsHelp}
                        />
                      </div>
                    </div>
                    <div className="preference-control storage-input-row">
                      <input
                        className="storage-input"
                        disabled={controller.updating}
                        id="max-items-input"
                        max="1000"
                        min="1"
                        onChange={event =>
                          setPendingMaxItemsInput(event.target.value)
                        }
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
                        onClick={event =>
                          void applyStorageLimit(event.currentTarget)
                        }
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
                      <div className="preference-label-with-help">
                        <label htmlFor="history-budget-input">
                          {messages.historyBudget}
                        </label>
                        <SettingsHelp
                          id="history-budget-help"
                          label={messages.settingHelp(messages.historyBudget)}
                          text={messages.historyBudgetHelp(
                            formatBytes(settings.max_event_bytes, language)
                          )}
                        />
                      </div>
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
                  <div className="preference-row preference-row-stacked">
                    <div className="preference-copy">
                      <div className="preference-label-with-help">
                        <label htmlFor="event-budget-input">
                          {messages.eventBudget}
                        </label>
                        <SettingsHelp
                          id="event-budget-help"
                          label={messages.settingHelp(messages.eventBudget)}
                          text={messages.eventBudgetHelp}
                        />
                      </div>
                    </div>
                    <div className="preference-control storage-input-row">
                      <input
                        aria-describedby={
                          isEventBudgetValid ? undefined : "event-budget-error"
                        }
                        aria-invalid={!isEventBudgetValid}
                        className="storage-input"
                        disabled={controller.updating}
                        id="event-budget-input"
                        max="256"
                        min="1"
                        step="1"
                        onChange={event =>
                          setPendingEventBudgetInput(event.target.value)
                        }
                        type="number"
                        value={pendingEventBudgetInput}
                      />
                      <span className="storage-unit">MiB</span>
                      <button
                        className="btn btn-primary"
                        disabled={
                          controller.updating ||
                          !isEventBudgetValid ||
                          !isEventBudgetDirty
                        }
                        onClick={() =>
                          void controller.updateMaxEventBytes(
                            parsedEventBudget * mebibyte
                          )
                        }
                        type="button"
                      >
                        {messages.apply}
                      </button>
                    </div>
                    {!isEventBudgetValid && (
                      <p
                        className="settings-error"
                        id="event-budget-error"
                        role="alert"
                      >
                        {messages.eventBudgetError}
                      </p>
                    )}
                  </div>
                </section>
                <h2 className="preference-group-heading">
                  {messages.clipboardHistory}
                </h2>
                <section className="preference-group">
                  <div className="preference-row settings-history-actions">
                    <span className="preference-copy">
                      <span className="preference-title">
                        {messages.clipboardHistory}
                      </span>
                      <span className="preference-description">
                        {messages.clearHistoryDescription(
                          settings.history_count
                        )}
                      </span>
                    </span>
                    <button
                      className="btn btn-danger settings-clear-button"
                      disabled={
                        clearingHistory ||
                        controller.updating ||
                        settings.history_count === 0
                      }
                      onClick={event => {
                        returnFocusRef.current = event.currentTarget;
                        setShowClearHistoryDialog(true);
                      }}
                      type="button"
                    >
                      <Trash2 aria-hidden="true" size={15} />
                      {clearingHistory
                        ? messages.clearingHistory
                        : messages.clearAll}
                    </button>
                  </div>
                </section>
              </div>
            )}
            {activeCategory === "menu-bar" && (
              <div className="settings-category-content settings-category-menu-bar">
                <section className="preference-group">
                  <label className="preference-row">
                    <span className="preference-copy">
                      <span className="preference-title">
                        {messages.showInMenuBar}
                      </span>
                    </span>
                    <span className="mac-switch">
                      <input
                        checked={settings.show_in_menu_bar}
                        disabled={controller.updating}
                        onChange={event =>
                          void controller.updateMenuBarVisibility(
                            event.target.checked
                          )
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
                    </div>
                    <div className="preference-control storage-input-row">
                      <input
                        className="storage-input"
                        disabled={controller.updating}
                        id="menu-bar-item-limit-input"
                        title={messages.menuBarItemLimitDescription(
                          settings.menu_bar_item_limit
                        )}
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
                          void controller.updateMenuBarItemLimit(
                            parsedMenuBarItemLimit
                          )
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
              </div>
            )}
          </div>
        </main>
      </div>

      {showConfirmDialog && (
        <div className="modal-overlay">
          <div
            aria-labelledby="reduce-history-title"
            aria-modal="true"
            className="modal-content"
            onKeyDown={handleDialogKeyDown}
            ref={dialogRef}
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
                data-dialog-cancel
                disabled={controller.updating}
                onClick={cancelDialog}
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

      {showClearHistoryDialog && (
        <div className="modal-overlay">
          <div
            aria-labelledby="clear-history-title"
            aria-modal="true"
            className="modal-content"
            onKeyDown={handleDialogKeyDown}
            ref={dialogRef}
            role="dialog"
          >
            <div className="modal-header">
              <AlertTriangle className="warning-icon" size={24} />
              <h3 id="clear-history-title">
                {messages.clearHistoryConfirmationTitle}
              </h3>
            </div>

            <div className="modal-body">
              <p>
                {messages.clearHistoryConfirmationDescription(
                  settings.history_count
                )}
              </p>
              <p className="warning-text">{messages.cannotUndo}</p>
            </div>

            <div className="modal-actions">
              <button
                className="btn btn-secondary"
                data-dialog-cancel
                onClick={cancelDialog}
                type="button"
              >
                {messages.cancel}
              </button>
              <button
                className="btn btn-danger"
                onClick={() => {
                  setShowClearHistoryDialog(false);
                  void clearAllEvents();
                }}
                type="button"
              >
                {messages.clearAll}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
