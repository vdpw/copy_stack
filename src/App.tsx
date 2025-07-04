import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Copy, Trash2, RefreshCw } from "lucide-react";
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

  const loadEvents = async () => {
    setLoading(true);
    try {
      const events = await invoke<StoredEvent[]>("get_copy_events");
      setCopyEvents(events);
    } catch (error) {
      console.error("Failed to load events:", error);
    } finally {
      setLoading(false);
    }
  };

  const deleteEvent = async (id: string) => {
    try {
      await invoke("delete_copy_event", { id });
      await loadEvents(); // Reload the list
    } catch (error) {
      console.error("Failed to delete event:", error);
    }
  };

  const copyToClipboard = async (eventData: string) => {
    try {
      await invoke("copy_to_clipboard", { eventData });
    } catch (error) {
      console.error("Failed to copy to clipboard:", error);
    }
  };

  const clearAllEvents = async () => {
    try {
      await invoke("clear_all_events");
      await loadEvents(); // Reload the list
    } catch (error) {
      console.error("Failed to clear events:", error);
    }
  };

  useEffect(() => {
    loadEvents();

    // Listen for new copy events
    const unlisten = listen("new-copy-event", (event) => {
      const newEvent = event.payload as ClipboardEvent;
      // Convert the event to StoredEvent format for consistency
      const storedEvent: StoredEvent = {
        id: crypto.randomUUID(),
        event_data: JSON.stringify(newEvent),
        timestamp: new Date().toISOString(),
      };
      setCopyEvents(prev => [storedEvent, ...prev]);
    });

    return () => {
      unlisten.then(f => f());
    };
  }, []);

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
          if (data.type === "public.utf8-plain-text" || data.type === "public.utf16-plain-text") {
            // Convert number array back to string
            const text = new TextDecoder().decode(new Uint8Array(data.data));
            return text;
          }
        }
      }
      
      // If no text found, show the first available data type
      if (event.items[0]?.data_list[0]) {
        return `[${event.items[0].data_list[0].type}]`;
      }
      
      return "Unknown content type";
    } catch (error) {
      console.error("Failed to parse event data:", error);
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
            onClick={loadEvents} 
            disabled={loading}
            className="btn btn-secondary"
          >
            <RefreshCw size={16} />
            Refresh
          </button>
          <button 
            onClick={clearAllEvents} 
            className="btn btn-danger"
          >
            <Trash2 size={16} />
            Clear All
          </button>
        </div>
      </header>

      <main className="main">
        {loading ? (
          <div className="loading">Loading...</div>
        ) : copyEvents.length === 0 ? (
          <div className="empty-state">
            <p>No copy events yet. Start copying text to see them here!</p>
            <p className="hint">💡 This is a desktop clipboard manager application</p>
          </div>
        ) : (
          <div className="events-list">
            {copyEvents.map((event) => {
              const content = getEventContent(event.event_data);
              return (
                <div key={event.id} className="event-card">
                  <div className="event-content">
                    <p className="event-text">{truncateContent(content)}</p>
                    <p className="event-timestamp">{formatTimestamp(event.timestamp)}</p>
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
