import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, Copy, RefreshCw, Settings, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import "./App.css";

interface Data {
  type: string;
  data: number[]; // Vec<u8> serialized as number array
}

interface Item {
  data_list: Data[];
}

interface ClipboardEvent {
  items: Item[];
}

interface StoredEvent {
  id: string;
  event_data: string; // JSON serialized ClipboardEvent
  timestamp: string;
}

function App() {
  const [copyEvents, setCopyEvents] = useState<StoredEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [maxItems, setMaxItems] = useState(100);
  const [showSettings, setShowSettings] = useState(false);
  const [settingsLoading, setSettingsLoading] = useState(false);
  const [showConfirmDialog, setShowConfirmDialog] = useState(false);
  const [pendingMaxItems, setPendingMaxItems] = useState(100);
  const [eventsToDelete, setEventsToDelete] = useState(0);

  const loadEvents = async () => {
    setLoading(true);
    try {
      const events = await invoke<StoredEvent[]>("get_copy_events");
      setCopyEvents(events);
    } catch (error) {
          } finally {
      setLoading(false);
    }
  };

  const loadMaxItems = async () => {
    try {
      const max = await invoke<number>("get_max_items");
      setMaxItems(max);
      setPendingMaxItems(max); // Also update pendingMaxItems to match
    } catch (error) {
          }
  };

  const updateMaxItems = async (newMaxItems: number) => {
    setSettingsLoading(true);
    try {
      await invoke("set_max_items", { maxItems: newMaxItems });
      setMaxItems(newMaxItems);
      await loadEvents(); // Reload events after changing limit
    } catch (error) {
          } finally {
      setSettingsLoading(false);
    }
  };

  const handleMaxItemsChange = (value: string) => {
    const numValue = parseInt(value);

    if (!isNaN(numValue) && numValue >= 1 && numValue <= 1000) {
      // Always update pendingMaxItems to reflect the input value
      setPendingMaxItems(numValue);

      const currentEventCount = copyEvents.length;

      if (numValue < currentEventCount) {
        // Show confirmation dialog
        setEventsToDelete(currentEventCount - numValue);
        setShowConfirmDialog(true);
      } else {
        // No confirmation needed, update directly
        updateMaxItems(numValue);
      }
    } else {
    }
  };

  const confirmMaxItemsChange = async () => {
    setShowConfirmDialog(false);
    await updateMaxItems(pendingMaxItems);
  };

  const cancelMaxItemsChange = () => {
    setShowConfirmDialog(false);
    setPendingMaxItems(maxItems);
  };

  const deleteEvent = async (id: string) => {
    try {
      await invoke("delete_copy_event", { id });
      await loadEvents(); // Reload the list
    } catch (error) {
          }
  };

  const copyToClipboard = async (eventData: string) => {
    try {
      await invoke("copy_to_clipboard", { eventData });
    } catch (error) {
          }
  };

  const clearAllEvents = async () => {
    try {
      await invoke("clear_all_events");
      await loadEvents(); // Reload the list
    } catch (error) {
          }
  };

  useEffect(() => {
    loadEvents();
    loadMaxItems();

    // Listen for new copy events
    const unlisten = listen("new-copy-event", event => {
      const newEvent = event.payload as ClipboardEvent;
      // Convert the event to StoredEvent format for consistency
      const storedEvent: StoredEvent = {
        id: crypto.randomUUID(),
        event_data: JSON.stringify(newEvent),
        timestamp: new Date().toISOString(),
      };
      setCopyEvents(prev => {
        const newEvents = [storedEvent, ...prev];
        // Enforce max_items limit in UI
        const limitedEvents = newEvents.slice(0, maxItems);
        return limitedEvents;
      });
    });

    return () => {
      unlisten.then(f => f());
    };
  }, [maxItems]);

  // Debug effect to track showConfirmDialog state
  useEffect(() => {}, [showConfirmDialog]);

  const formatTimestamp = (timestamp: string) => {
    return new Date(timestamp).toLocaleString();
  };

  const getEventContent = (eventData: string): string => {
    try {
      const event: ClipboardEvent = JSON.parse(eventData);
      if (event.items.length === 0) return "Empty clipboard";

      // Try to find text content first
      for (const item of event.items) {
        for (const data of item.data_list) {
          if (
            data.type === "public.utf8-plain-text" ||
            data.type === "public.utf16-plain-text"
          ) {
            // Convert number array back to string
            const text = new TextDecoder().decode(new Uint8Array(data.data));
            return text;
          }
        }
      }

      // If no text found, show the first available data type
      if (event.items[0]?.data_list[0]) {
        const dataType = event.items[0].data_list[0].type;
        return `[${dataType}]`;
      }

      return "Unknown content type";
    } catch (error) {
            return "Error parsing content";
    }
  };

  const truncateContent = (content: string, maxLength: number = 100) => {
    if (content.length <= maxLength) return content;
    return content.substring(0, maxLength) + "...";
  };

  return (
    <div className="app">
      <header className="header">
        <h1>Copy Stack</h1>
        <div className="header-actions">
          <button
            onClick={() => setShowSettings(!showSettings)}
            className="btn btn-secondary"
            title="Settings"
          >
            <Settings size={16} />
            Settings
          </button>
          <button
            onClick={loadEvents}
            disabled={loading}
            className="btn btn-secondary"
          >
            <RefreshCw size={16} />
            Refresh
          </button>
          <button onClick={clearAllEvents} className="btn btn-danger">
            <Trash2 size={16} />
            Clear All
          </button>
        </div>
      </header>

      {showSettings && (
        <div className="settings-panel">
          <div className="settings-content">
            <h3>Settings</h3>
            <div className="setting-item">
              <label htmlFor="maxItems">Maximum items to store:</label>
              <div className="setting-control">
                <input
                  id="maxItems"
                  type="number"
                  min="1"
                  max="1000"
                  value={pendingMaxItems}
                  onChange={e => handleMaxItemsChange(e.target.value)}
                  disabled={settingsLoading}
                  className="setting-input"
                />
                <span className="setting-hint">(1-1000)</span>
              </div>
            </div>
            <div className="setting-info">
              <p>
                Current events: {copyEvents.length} / {maxItems}
              </p>
            </div>
          </div>
        </div>
      )}

      {showConfirmDialog && (
        <div className="modal-overlay">
          <div className="modal-content">
            <div className="modal-header">
              <AlertTriangle size={24} className="warning-icon" />
              <h3>Confirm Action</h3>
            </div>
            <div className="modal-body">
              <p>
                You are about to reduce the maximum items from{" "}
                <strong>{maxItems}</strong> to{" "}
                <strong>{pendingMaxItems}</strong>.
              </p>
              <p>
                This will permanently delete the{" "}
                <strong>{eventsToDelete} oldest events</strong> from your
                clipboard history.
              </p>
              <p className="warning-text">This action cannot be undone!</p>
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
                onClick={confirmMaxItemsChange}
                className="btn btn-danger"
                disabled={settingsLoading}
              >
                {settingsLoading ? "Updating..." : "Delete & Update"}
              </button>
            </div>
          </div>
        </div>
      )}

      <main className="main">
        {loading ? (
          <div className="loading">Loading...</div>
        ) : copyEvents.length === 0 ? (
          <div className="empty-state">
            <p>No copy events yet. Start copying text to see them here!</p>
            <p className="hint">
              💡 This is a desktop clipboard manager application
            </p>
          </div>
        ) : (
          <div className="events-list">
            {copyEvents.map(event => {
              const content = getEventContent(event.event_data);
              return (
                <div key={event.id} className="event-card">
                  <div className="event-content">
                    <p className="event-text">{truncateContent(content)}</p>
                    <p className="event-timestamp">
                      {formatTimestamp(event.timestamp)}
                    </p>
                  </div>
                  <div className="event-actions">
                    <button
                      onClick={() => copyToClipboard(event.event_data)}
                      className="btn btn-primary"
                      title="Copy to clipboard"
                    >
                      <Copy size={16} />
                    </button>
                    <button
                      onClick={() => deleteEvent(event.id)}
                      className="btn btn-danger"
                      title="Delete event"
                    >
                      <Trash2 size={16} />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </main>
    </div>
  );
}

export default App;
