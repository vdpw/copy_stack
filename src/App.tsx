import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  ArrowUpDown,
  Archive,
  Copy,
  Eye,
  EyeOff,
  RefreshCw,
  Settings2,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import "./App.css";

type View = "history" | "settings";

interface Data {
  type: string;
  data: number[];
}

interface Item {
  data_list: Data[];
}

interface ClipboardEvent {
  items: Item[];
}

interface StoredEvent {
  id: string;
  event_data: string;
  timestamp: string;
}

interface AppSettings {
  max_items: number;
  show_in_menu_bar: boolean;
  move_restored_item_to_top: boolean;
}

function App() {
  const [activeView, setActiveView] = useState<View>("history");
  const [copyEvents, setCopyEvents] = useState<StoredEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [settingsLoading, setSettingsLoading] = useState(false);
  const [maxItems, setMaxItems] = useState(100);
  const [pendingMaxItemsInput, setPendingMaxItemsInput] = useState("100");
  const [menuBarVisible, setMenuBarVisible] = useState(true);
  const [moveRestoredItemToTop, setMoveRestoredItemToTop] = useState(false);
  const [showConfirmDialog, setShowConfirmDialog] = useState(false);
  const [eventsToDelete, setEventsToDelete] = useState(0);

  const loadEvents = useCallback(async () => {
    setLoading(true);
    try {
      const events = await invoke<StoredEvent[]>("get_copy_events");
      setCopyEvents(events);
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to load clipboard history", error);
      }
    } finally {
      setLoading(false);
    }
  }, []);

  const loadSettings = useCallback(async () => {
    try {
      const settings = await invoke<AppSettings>("get_app_settings");
      setMaxItems(settings.max_items);
      setPendingMaxItemsInput(String(settings.max_items));
      setMenuBarVisible(settings.show_in_menu_bar);
      setMoveRestoredItemToTop(settings.move_restored_item_to_top);
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to load app settings", error);
      }
    }
  }, []);

  useEffect(() => {
    void loadEvents();
    void loadSettings();

    let unlistenHistory: (() => void) | undefined;
    let unlistenNavigation: (() => void) | undefined;

    const registerListeners = async () => {
      unlistenHistory = await listen("clipboard-history-updated", () => {
        void loadEvents();
      });
      unlistenNavigation = await listen<string>("app:navigate", event => {
        setActiveView(event.payload === "settings" ? "settings" : "history");
      });
    };

    void registerListeners();

    return () => {
      unlistenHistory?.();
      unlistenNavigation?.();
    };
  }, [loadEvents, loadSettings]);

  const parsedPendingMaxItems = Number.parseInt(pendingMaxItemsInput, 10);
  const isPendingMaxItemsValid =
    Number.isInteger(parsedPendingMaxItems) &&
    parsedPendingMaxItems >= 1 &&
    parsedPendingMaxItems <= 1000;
  const isStorageLimitDirty =
    isPendingMaxItemsValid && parsedPendingMaxItems !== maxItems;

  const updateMaxItems = async (newMaxItems: number) => {
    setSettingsLoading(true);
    try {
      await invoke("set_max_items", { maxItems: newMaxItems });
      setMaxItems(newMaxItems);
      setPendingMaxItemsInput(String(newMaxItems));
      await loadEvents();
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to update max items", error);
      }
    } finally {
      setSettingsLoading(false);
    }
  };

  const updateMenuBarVisibility = async (nextVisible: boolean) => {
    setSettingsLoading(true);
    try {
      await invoke("set_show_in_menu_bar", {
        showInMenuBar: nextVisible,
      });
      setMenuBarVisible(nextVisible);
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to update menu bar visibility", error);
      }
    } finally {
      setSettingsLoading(false);
    }
  };

  const updateRestoreOrdering = async (nextEnabled: boolean) => {
    setSettingsLoading(true);
    try {
      await invoke("set_move_restored_item_to_top", {
        moveRestoredItemToTop: nextEnabled,
      });
      setMoveRestoredItemToTop(nextEnabled);
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to update restore ordering", error);
      }
    } finally {
      setSettingsLoading(false);
    }
  };

  const handleApplyStorageLimit = async () => {
    if (!isPendingMaxItemsValid || !isStorageLimitDirty) {
      return;
    }

    if (parsedPendingMaxItems < copyEvents.length) {
      setEventsToDelete(copyEvents.length - parsedPendingMaxItems);
      setShowConfirmDialog(true);
      return;
    }

    await updateMaxItems(parsedPendingMaxItems);
  };

  const confirmMaxItemsChange = async () => {
    setShowConfirmDialog(false);
    if (!isPendingMaxItemsValid) {
      return;
    }
    await updateMaxItems(parsedPendingMaxItems);
  };

  const cancelMaxItemsChange = () => {
    setShowConfirmDialog(false);
    setPendingMaxItemsInput(String(maxItems));
  };

  const deleteEvent = async (id: string) => {
    try {
      await invoke("delete_copy_event", { id });
      await loadEvents();
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to delete clipboard item", error);
      }
    }
  };

  const copyToClipboard = async (id: string) => {
    if (import.meta.env.DEV) {
      console.info("[copy_stack] restore requested from UI", { id });
    }
    try {
      await invoke("copy_to_clipboard", { id });
      if (import.meta.env.DEV) {
        console.info("[copy_stack] restore command completed", { id });
      }
      await loadEvents();
      if (import.meta.env.DEV) {
        console.info("[copy_stack] history refreshed after restore", { id });
      }
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("[copy_stack] failed to restore clipboard item", {
          id,
          error,
        });
      }
    }
  };

  const clearAllEvents = async () => {
    try {
      await invoke("clear_all_events");
      await loadEvents();
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to clear clipboard history", error);
      }
    }
  };

  const formatTimestamp = (timestamp: string) => {
    return new Date(timestamp).toLocaleString();
  };

  const getEventContent = (eventData: string): string => {
    try {
      const event: ClipboardEvent = JSON.parse(eventData);
      if (event.items.length === 0) {
        return "Empty clipboard";
      }

      for (const item of event.items) {
        for (const data of item.data_list) {
          if (
            data.type === "public.utf8-plain-text" ||
            data.type === "public.utf16-plain-text" ||
            data.type === "NSStringPboardType"
          ) {
            const decoder =
              data.type === "public.utf16-plain-text"
                ? new TextDecoder("utf-16le")
                : new TextDecoder();
            const text = decoder.decode(new Uint8Array(data.data));
            if (text.trim().length > 0) {
              return text;
            }
          }
        }
      }

      if (event.items[0]?.data_list[0]) {
        return `[${event.items[0].data_list[0].type}]`;
      }

      return "Unknown content type";
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to parse clipboard payload", error);
      }
      return "Error parsing content";
    }
  };

  const truncateContent = (content: string, maxLength = 160) => {
    const flattened = content.replace(/\s+/g, " ").trim();
    if (flattened.length <= maxLength) {
      return flattened;
    }
    return `${flattened.slice(0, maxLength)}...`;
  };

  return (
    <div className="app-shell">
      <header className="hero">
        <div className="hero-copy">
          <p className="eyebrow">macOS clipboard stack</p>
          <h1>Copy Stack</h1>
          <p className="hero-description">
            Run in the menu bar, browse recent clipboard events, and keep local
            history trimmed to the size you want.
          </p>
        </div>

        <div className="hero-stats">
          <div className="stat-card">
            <span className="stat-label">Stored items</span>
            <strong>{copyEvents.length}</strong>
            <span className="stat-detail">Up to {maxItems} saved locally</span>
          </div>
          <div className="stat-card">
            <span className="stat-label">Menu bar</span>
            <strong>{menuBarVisible ? "Visible" : "Hidden"}</strong>
            <span className="stat-detail">
              {menuBarVisible
                ? "Recent clips are available from the tray menu."
                : "Turn it back on from Settings when needed."}
            </span>
          </div>
        </div>
      </header>

      <div className="workspace">
        <aside className="side-nav">
          <button
            className={`nav-button ${
              activeView === "history" ? "is-active" : ""
            }`}
            onClick={() => setActiveView("history")}
          >
            <Archive size={18} />
            History
          </button>
          <button
            className={`nav-button ${
              activeView === "settings" ? "is-active" : ""
            }`}
            onClick={() => setActiveView("settings")}
          >
            <Settings2 size={18} />
            Settings
          </button>

          <div className="side-note">
            Every stored clip is mirrored into the menu bar as a direct action,
            so selecting a tray item restores it back to the clipboard.
          </div>
        </aside>

        <main className="content-panel">
          {activeView === "history" ? (
            <>
              <section className="panel-header">
                <div>
                  <p className="section-kicker">Clipboard history</p>
                  <h2>Recent events</h2>
                  <p className="section-description">
                    Refresh the list, restore an item, or clear the local stack.
                  </p>
                </div>

                <div className="panel-actions">
                  <button
                    onClick={() => void loadEvents()}
                    disabled={loading}
                    className="btn btn-secondary"
                  >
                    <RefreshCw size={16} />
                    Refresh
                  </button>
                  <button
                    onClick={() => void clearAllEvents()}
                    className="btn btn-danger"
                    disabled={copyEvents.length === 0}
                  >
                    <Trash2 size={16} />
                    Clear all
                  </button>
                </div>
              </section>

              {loading ? (
                <div className="placeholder-card">Loading clipboard history...</div>
              ) : copyEvents.length === 0 ? (
                <div className="empty-state">
                  <h3>No clipboard events yet</h3>
                  <p>
                    Start copying text or files and they will appear here and in
                    the menu bar menu.
                  </p>
                </div>
              ) : (
                <div className="events-list">
                  {copyEvents.map(event => {
                    const content = getEventContent(event.event_data);
                    return (
                      <article key={event.id} className="event-card">
                        <div className="event-content">
                          <p className="event-text">
                            {truncateContent(content)}
                          </p>
                          <p className="event-timestamp">
                            {formatTimestamp(event.timestamp)}
                          </p>
                        </div>

                        <div className="event-actions">
                          <button
                            onClick={() => void copyToClipboard(event.id)}
                            className="btn btn-primary"
                            title="Restore to clipboard"
                          >
                            <Copy size={16} />
                          </button>
                          <button
                            onClick={() => void deleteEvent(event.id)}
                            className="btn btn-danger"
                            title="Delete item"
                          >
                            <Trash2 size={16} />
                          </button>
                        </div>
                      </article>
                    );
                  })}
                </div>
              )}
            </>
          ) : (
            <>
              <section className="panel-header">
                <div>
                  <p className="section-kicker">Configuration</p>
                  <h2>Settings</h2>
                  <p className="section-description">
                    Control how much history stays on device and whether the app
                    is shown in the macOS menu bar.
                  </p>
                </div>
              </section>

              <div className="settings-grid">
                <article className="settings-card">
                  <div className="settings-card-header">
                    <div>
                      <p className="settings-label">Local retention</p>
                      <h3>Stored event limit</h3>
                    </div>
                  </div>

                  <p className="settings-description">
                    Reduce or expand the number of clipboard events kept in the
                    local SQLite store. Lowering the limit immediately removes
                    the oldest items after confirmation.
                  </p>

                  <div className="storage-control">
                    <label htmlFor="max-items-input">Maximum saved events</label>
                    <div className="storage-input-row">
                      <input
                        id="max-items-input"
                        type="number"
                        min="1"
                        max="1000"
                        value={pendingMaxItemsInput}
                        onChange={event =>
                          setPendingMaxItemsInput(event.target.value)
                        }
                        disabled={settingsLoading}
                        className="storage-input"
                      />
                      <button
                        className="btn btn-primary"
                        onClick={() => void handleApplyStorageLimit()}
                        disabled={
                          settingsLoading ||
                          !isPendingMaxItemsValid ||
                          !isStorageLimitDirty
                        }
                      >
                        Apply
                      </button>
                    </div>

                    <p className="settings-helper">
                      Currently storing {copyEvents.length} of {maxItems} events.
                    </p>
                    {!isPendingMaxItemsValid && (
                      <p className="settings-error">
                        Enter a whole number between 1 and 1000.
                      </p>
                    )}
                  </div>
                </article>

                <article className="settings-card">
                  <div className="settings-card-header">
                    <div>
                      <p className="settings-label">List order</p>
                      <h3>Restore ordering</h3>
                    </div>
                  </div>

                  <p className="settings-description">
                    Choose whether restoring a stored clipboard item keeps its
                    current position or moves it back to the top of history.
                  </p>

                  <button
                    className={`toggle-button ${
                      moveRestoredItemToTop ? "is-on" : "is-off"
                    }`}
                    onClick={() =>
                      void updateRestoreOrdering(!moveRestoredItemToTop)
                    }
                    disabled={settingsLoading}
                    role="switch"
                    aria-checked={moveRestoredItemToTop}
                  >
                    <span className="toggle-track">
                      <span className="toggle-thumb" />
                    </span>
                    <span className="toggle-copy">
                      <strong>
                        {moveRestoredItemToTop
                          ? "Move restored items to top"
                          : "Keep restored items in place"}
                      </strong>
                      <span>
                        <ArrowUpDown size={14} />
                        {moveRestoredItemToTop
                          ? "Copy actions refresh list order."
                          : "Copy actions preserve list order."}
                      </span>
                    </span>
                  </button>
                </article>

                <article className="settings-card">
                  <div className="settings-card-header">
                    <div>
                      <p className="settings-label">Menu bar</p>
                      <h3>Tray visibility</h3>
                    </div>
                  </div>

                  <p className="settings-description">
                    Hide or show Copy Stack in the macOS menu bar. When it is
                    visible, each recent clipboard event appears as its own tray
                    menu item.
                  </p>

                  <button
                    className={`toggle-button ${
                      menuBarVisible ? "is-on" : "is-off"
                    }`}
                    onClick={() =>
                      void updateMenuBarVisibility(!menuBarVisible)
                    }
                    disabled={settingsLoading}
                    role="switch"
                    aria-checked={menuBarVisible}
                  >
                    <span className="toggle-track">
                      <span className="toggle-thumb" />
                    </span>
                    <span className="toggle-copy">
                      <strong>
                        {menuBarVisible
                          ? "Shown in the menu bar"
                          : "Hidden from the menu bar"}
                      </strong>
                      <span>
                        {menuBarVisible ? (
                          <>
                            <Eye size={14} />
                            Tray menu is active.
                          </>
                        ) : (
                          <>
                            <EyeOff size={14} />
                            Re-open the window from the Dock to turn it back on.
                          </>
                        )}
                      </span>
                    </span>
                  </button>
                </article>

                <article className="settings-card settings-card-accent">
                  <div className="settings-card-header">
                    <div>
                      <p className="settings-label">Current status</p>
                      <h3>What changes immediately</h3>
                    </div>
                  </div>

                  <ul className="status-list">
                    <li>
                      New clipboard events update the app window and tray menu
                      automatically.
                    </li>
                    <li>
                      Clearing or deleting history refreshes the menu-bar list
                      right away.
                    </li>
                    <li>
                      Restoring an item keeps or updates its position based on
                      the restore ordering setting.
                    </li>
                    <li>
                      Closing the main window keeps the app running so the menu
                      bar entry can stay available.
                    </li>
                  </ul>
                </article>
              </div>
            </>
          )}
        </main>
      </div>

      {showConfirmDialog && (
        <div className="modal-overlay">
          <div className="modal-content">
            <div className="modal-header">
              <AlertTriangle size={24} className="warning-icon" />
              <h3>Reduce stored history?</h3>
            </div>

            <div className="modal-body">
              <p>
                Changing the storage limit from <strong>{maxItems}</strong> to{" "}
                <strong>{parsedPendingMaxItems}</strong> will remove the{" "}
                <strong>{eventsToDelete}</strong> oldest clipboard events from
                local storage.
              </p>
              <p className="warning-text">This action cannot be undone.</p>
            </div>

            <div className="modal-actions">
              <button
                onClick={cancelMaxItemsChange}
                className="btn btn-secondary"
                disabled={settingsLoading}
              >
                Cancel
              </button>
              <button
                onClick={() => void confirmMaxItemsChange()}
                className="btn btn-danger"
                disabled={settingsLoading}
              >
                {settingsLoading ? "Updating..." : "Delete and update"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
