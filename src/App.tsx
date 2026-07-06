import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  AlertTriangle,
  AppWindow,
  ArrowUpDown,
  Copy,
  Eye,
  EyeOff,
  File,
  Files,
  Folder,
  Image as ImageIcon,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type { KeyboardEvent } from "react";
import "./App.css";

const currentWindowLabel = getCurrentWindow().label;
const isSettingsWindow = currentWindowLabel === "settings";
const fileDisplayFormat = "copy_stack.file-items.v1";
const displayMaxWidth = 40;
const truncationSuffix = "...";
const pngSignature = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

interface StoredEvent {
  content_hash: string;
  data_type: string;
  display: number[];
  timestamp: number;
  source_app: string | null;
}

interface AppSettings {
  max_items: number;
  show_in_menu_bar: boolean;
  move_restored_item_to_top: boolean;
}

interface FileDisplayItem {
  type: string;
  name: string;
}

interface FileDisplayPayload {
  format: string;
  items: FileDisplayItem[];
}

interface DisplayPreview {
  text: string;
  fileItems: FileDisplayItem[] | null;
  image: ImageDisplay | null;
}

interface ImageDisplay {
  bytes: Uint8Array;
  mediaType: string;
  label: string;
}

function ImageThumbnail({ bytes, label, mediaType }: ImageDisplay) {
  const [imageUrl, setImageUrl] = useState<string | null>(null);

  useEffect(() => {
    const blob = new window.Blob([bytes], { type: mediaType });
    const nextImageUrl = window.URL.createObjectURL(blob);
    setImageUrl(nextImageUrl);

    return () => {
      window.URL.revokeObjectURL(nextImageUrl);
    };
  }, [bytes, mediaType]);

  if (!imageUrl) {
    return <div aria-hidden="true" className="event-image-placeholder" />;
  }

  return (
    <img
      alt={`${label} thumbnail`}
      className="event-image-thumbnail"
      draggable={false}
      src={imageUrl}
    />
  );
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
  const [expandedEventHashes, setExpandedEventHashes] = useState<Set<string>>(
    () => new Set(),
  );

  const loadEvents = useCallback(async () => {
    setLoading(true);
    try {
      const events = await invoke<StoredEvent[]>("get_copy_events");
      setCopyEvents(events);
      setExpandedEventHashes(current => {
        const currentHashes = new Set(events.map(event => event.content_hash));
        const next = new Set(
          Array.from(current).filter(contentHash =>
            currentHashes.has(contentHash),
          ),
        );
        return next.size === current.size ? current : next;
      });
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

  const toggleEventExpansion = (contentHash: string) => {
    setExpandedEventHashes(current => {
      const next = new Set(current);
      if (next.has(contentHash)) {
        next.delete(contentHash);
      } else {
        next.add(contentHash);
      }
      return next;
    });
  };

  const handleEventCardKeyDown = (
    event: KeyboardEvent<HTMLElement>,
    contentHash: string,
  ) => {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }

    event.preventDefault();
    toggleEventExpansion(contentHash);
  };

  const formatTimestamp = (timestamp: number) => {
    return new Date(timestamp).toLocaleString();
  };

  const getCharacterDisplayWidth = (character: string) => {
    if (
      /[\u1100-\u115F\u2329\u232A\u2E80-\uA4CF\uAC00-\uD7A3\uF900-\uFAFF\uFE10-\uFE19\uFE30-\uFE6F\uFF00-\uFF60\uFFE0-\uFFE6\u{1F300}-\u{1FAFF}]/u.test(
        character,
      )
    ) {
      return 2;
    }

    return 1;
  };

  const getDisplayWidth = (content: string) => {
    return Array.from(content).reduce(
      (width, character) => width + getCharacterDisplayWidth(character),
      0,
    );
  };

  const truncateContent = (content: string, maxWidth = displayMaxWidth) => {
    const flattened = content.replace(/\s+/g, " ").trim();
    if (getDisplayWidth(flattened) <= maxWidth) {
      return flattened;
    }

    const suffixWidth = getDisplayWidth(truncationSuffix);
    const availableWidth = Math.max(0, maxWidth - suffixWidth);
    let truncated = "";
    let currentWidth = 0;

    for (const character of Array.from(flattened)) {
      const characterWidth = getCharacterDisplayWidth(character);
      if (currentWidth + characterWidth > availableWidth) {
        break;
      }

      truncated += character;
      currentWidth += characterWidth;
    }

    return `${truncated}${truncationSuffix}`;
  };

  const decodeDisplayText = (event: StoredEvent) => {
    const text = new TextDecoder().decode(new Uint8Array(event.display));
    if (text.includes("\uFFFD")) {
      return event.data_type.toUpperCase();
    }
    return text;
  };

  const parseFileDisplay = (text: string) => {
    try {
      const parsed = JSON.parse(text) as Partial<FileDisplayPayload>;
      if (
        parsed.format !== fileDisplayFormat ||
        !Array.isArray(parsed.items)
      ) {
        return null;
      }

      const items = parsed.items.filter(
        (item): item is FileDisplayItem => {
          const candidate = item as Partial<FileDisplayItem> | null;
          return (
            typeof candidate?.type === "string" &&
            typeof candidate?.name === "string"
          );
        },
      );
      return items.length > 0 ? items : null;
    } catch {
      return null;
    }
  };

  const isPngDisplay = (display: number[]) => {
    return pngSignature.every((byte, index) => display[index] === byte);
  };

  const parseImageDisplay = (event: StoredEvent): ImageDisplay | null => {
    if (event.data_type !== "png" || !isPngDisplay(event.display)) {
      return null;
    }

    return {
      bytes: new Uint8Array(event.display),
      mediaType: "image/png",
      label: "PNG image",
    };
  };

  const getDisplayPreview = (event: StoredEvent): DisplayPreview => {
    const image = parseImageDisplay(event);
    const text = decodeDisplayText(event);
    const fileItems = parseFileDisplay(text);
    return {
      text: image ? image.label : text,
      fileItems,
      image,
    };
  };

  const renderFileItemIcon = (itemType: string) => {
    if (itemType === "folder") {
      return <Folder aria-hidden="true" className="event-type-icon" size={18} />;
    }

    return <File aria-hidden="true" className="event-type-icon" size={18} />;
  };

  const renderEventTypeIcon = (dataType: string) => {
    switch (dataType) {
      case "file":
        return <File aria-hidden="true" className="event-type-icon" size={18} />;
      case "folder":
        return (
          <Folder aria-hidden="true" className="event-type-icon" size={18} />
        );
      case "png":
      case "jpg":
      case "jpeg":
      case "gif":
      case "webp":
      case "tiff":
      case "tif":
      case "bmp":
      case "heic":
      case "heif":
        return (
          <ImageIcon aria-hidden="true" className="event-type-icon" size={18} />
        );
      case "files":
      case "files and folders":
        return <Files aria-hidden="true" className="event-type-icon" size={18} />;
      case "folders":
        return (
          <Folder aria-hidden="true" className="event-type-icon" size={18} />
        );
      default:
        return null;
    }
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
                  const preview = getDisplayPreview(event);
                  const isExpanded = expandedEventHashes.has(event.content_hash);
                  const visibleFileItems =
                    preview.fileItems && !isExpanded
                      ? preview.fileItems.slice(0, 1)
                      : preview.fileItems;
                  return (
                    <article
                      key={event.content_hash}
                      className={`event-card ${
                        isExpanded ? "event-card-expanded" : ""
                      }`}
                      role="button"
                      tabIndex={0}
                      aria-expanded={isExpanded}
                      onClick={() => toggleEventExpansion(event.content_hash)}
                      onKeyDown={keyboardEvent =>
                        handleEventCardKeyDown(
                          keyboardEvent,
                          event.content_hash,
                        )
                      }
                    >
                      <div className="event-content">
                        <p className="event-meta">
                          <span>{event.data_type}</span>
                          {event.source_app && (
                            <span className="event-source">
                              <AppWindow size={12} />
                              {event.source_app}
                            </span>
                          )}
                        </p>
                        {preview.image ? (
                          <div className="event-image-preview">
                            <ImageThumbnail {...preview.image} />
                            <p className="event-text">{preview.image.label}</p>
                          </div>
                        ) : visibleFileItems ? (
                          <ul className="event-file-items">
                            {visibleFileItems.map((item, index) => {
                              const hiddenItemCount =
                                preview.fileItems && !isExpanded
                                  ? preview.fileItems.length -
                                    visibleFileItems.length
                                  : 0;
                              const collapsedSuffix =
                                hiddenItemCount > 0
                                  ? ` + ${hiddenItemCount} more`
                                  : "";
                              const collapsedFileLabel =
                                hiddenItemCount > 0
                                  ? `${truncateContent(
                                      item.name,
                                      Math.max(
                                        0,
                                        displayMaxWidth -
                                          getDisplayWidth(collapsedSuffix),
                                      ),
                                    )}${collapsedSuffix}`
                                  : truncateContent(item.name);

                              return (
                                <li
                                  className="event-file-item"
                                  key={`${item.type}-${item.name}-${index}`}
                                >
                                  {renderFileItemIcon(item.type)}
                                  <span>
                                    {isExpanded
                                      ? item.name
                                      : collapsedFileLabel}
                                  </span>
                                </li>
                              );
                            })}
                          </ul>
                        ) : (
                          <div className="event-preview">
                            {renderEventTypeIcon(event.data_type)}
                            <p className="event-text">
                              {isExpanded
                                ? preview.text
                                : truncateContent(preview.text)}
                            </p>
                          </div>
                        )}
                        <p className="event-timestamp">
                          {formatTimestamp(event.timestamp)}
                        </p>
                      </div>

                      <div className="event-actions">
                        <button
                          onClick={clickEvent => {
                            clickEvent.stopPropagation();
                            void copyToClipboard(event.content_hash);
                          }}
                          className="btn btn-primary"
                          title="Restore to clipboard"
                        >
                          <Copy size={16} />
                        </button>
                        <button
                          onClick={clickEvent => {
                            clickEvent.stopPropagation();
                            void deleteEvent(event.content_hash);
                          }}
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
