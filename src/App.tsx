import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  AlertTriangle,
  ArrowUpDown,
  Copy,
  Eye,
  EyeOff,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import "./App.css";

const currentWindowLabel = getCurrentWindow().label;
const isSettingsWindow = currentWindowLabel === "settings";

interface StoredEvent {
  content_hash: string;
  data_type: string;
  display: number[];
  timestamp: number;
}

interface AppSettings {
  max_items: number;
  show_in_menu_bar: boolean;
  move_restored_item_to_top: boolean;
}

function App() {
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

    const registerListeners = async () => {
      unlistenHistory = await listen("clipboard-history-updated", () => {
        void loadEvents();
      });
    };

    void registerListeners();

    return () => {
      unlistenHistory?.();
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

  const deleteEvent = async (contentHash: string) => {
    try {
      await invoke("delete_copy_event", { contentHash });
      await loadEvents();
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("Failed to delete clipboard item", error);
      }
    }
  };

  const copyToClipboard = async (contentHash: string) => {
    if (import.meta.env.DEV) {
      console.info("[copy_stack] restore requested from UI", { contentHash });
    }
    try {
      await invoke("copy_to_clipboard", { contentHash });
      if (import.meta.env.DEV) {
        console.info("[copy_stack] restore command completed", { contentHash });
      }
      await loadEvents();
      if (import.meta.env.DEV) {
        console.info("[copy_stack] history refreshed after restore", {
          contentHash,
        });
      }
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error("[copy_stack] failed to restore clipboard item", {
          contentHash,
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

  const formatTimestamp = (timestamp: number) => {
    return new Date(timestamp).toLocaleString();
  };

  const truncateContent = (content: string, maxLength = 160) => {
    const flattened = content.replace(/\s+/g, " ").trim();
    if (flattened.length <= maxLength) {
      return flattened;
    }
    return `${flattened.slice(0, maxLength)}...`;
  };

  const getDisplayText = (event: StoredEvent) => {
    const text = new TextDecoder().decode(new Uint8Array(event.display));
    if (text.includes("\uFFFD")) {
      return event.data_type.toUpperCase();
    }
    return text;
  };

  return (
    <div className={`app-shell ${isSettingsWindow ? "settings-shell" : ""}`}>
      {isSettingsWindow ? (
        <main className="preferences-panel">
          <header className="preferences-header">
            <h1>Settings</h1>
          </header>

          <section className="preference-group">
            <div className="preference-row preference-row-stacked">
              <div className="preference-copy">
                <label htmlFor="max-items-input">Stored items</label>
                <p>
                  Keep the newest {maxItems} clips. Currently storing{" "}
                  {copyEvents.length}.
                </p>
              </div>
              <div className="preference-control storage-input-row">
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
              {!isPendingMaxItemsValid && (
                <p className="settings-error">
                  Enter a whole number between 1 and 1000.
                </p>
              )}
            </div>

            <label className="preference-row">
              <span className="preference-copy">
                <span className="preference-title">Move restored items to top</span>
                <span className="preference-description">
                  <ArrowUpDown size={13} />
                  {moveRestoredItemToTop
                    ? "Restored clips refresh history order."
                    : "Restored clips keep their current order."}
                </span>
              </span>
              <span className="mac-switch">
                <input
                  type="checkbox"
                  checked={moveRestoredItemToTop}
                  onChange={event =>
                    void updateRestoreOrdering(event.target.checked)
                  }
                  disabled={settingsLoading}
                />
                <span className="mac-switch-track" />
              </span>
            </label>

            <label className="preference-row">
              <span className="preference-copy">
                <span className="preference-title">Show in menu bar</span>
                <span className="preference-description">
                  {menuBarVisible ? <Eye size={13} /> : <EyeOff size={13} />}
                  {menuBarVisible
                    ? "Recent clips are available from the tray menu."
                    : "The tray menu is hidden."}
                </span>
              </span>
              <span className="mac-switch">
                <input
                  type="checkbox"
                  checked={menuBarVisible}
                  onChange={event =>
                    void updateMenuBarVisibility(event.target.checked)
                  }
                  disabled={settingsLoading}
                />
                <span className="mac-switch-track" />
              </span>
            </label>
          </section>
        </main>
      ) : (
        <div className="workspace">
          <main className="content-panel">
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
                  return (
                    <article key={event.content_hash} className="event-card">
                      <div className="event-content">
                        <p className="event-meta">
                          <span>{event.data_type}</span>
                        </p>
                        <p className="event-text">
                          {truncateContent(getDisplayText(event))}
                        </p>
                        <p className="event-timestamp">
                          {formatTimestamp(event.timestamp)}
                        </p>
                      </div>

                      <div className="event-actions">
                        <button
                          onClick={() => void copyToClipboard(event.content_hash)}
                          className="btn btn-primary"
                          title="Restore to clipboard"
                        >
                          <Copy size={16} />
                        </button>
                        <button
                          onClick={() => void deleteEvent(event.content_hash)}
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
          </main>
        </div>
      )}

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
