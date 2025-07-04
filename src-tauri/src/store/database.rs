use chrono::{DateTime, Utc};
use copy_event_listener::event::Event;
use rusqlite::{Connection, Result};
use serde_json;
use std::sync::Mutex;
use tauri::AppHandle;
use uuid::Uuid;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredEvent {
    pub id: String,
    pub event_data: String, // JSON serialized Event
    pub timestamp: DateTime<Utc>,
}

impl StoredEvent {
    pub fn new(event: Event) -> Result<Self, serde_json::Error> {
        Ok(Self {
            id: Uuid::new_v4().to_string(),
            event_data: serde_json::to_string(&event)?,
            timestamp: Utc::now(),
        })
    }
}

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new(_app_handle: &AppHandle) -> Result<Self> {
        // For now, use a simple path in the current directory
        let db_path = std::path::Path::new("copy_stack.db");
        let conn = Connection::open(db_path)?;

        // Create tables for storing copy_event_listener::event::Event
        conn.execute(
            "CREATE TABLE IF NOT EXISTS clipboard_events (
                id TEXT PRIMARY KEY,
                event_data TEXT NOT NULL,
                timestamp TEXT NOT NULL
            )",
            [],
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert_event(&self, event: &Event) -> Result<()> {
        let stored_event = StoredEvent::new(event.clone())
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO clipboard_events (id, event_data, timestamp) VALUES (?1, ?2, ?3)",
            (
                &stored_event.id,
                &stored_event.event_data,
                &stored_event.timestamp.to_rfc3339(),
            ),
        )?;
        Ok(())
    }

    pub fn get_all_events(&self) -> Result<Vec<StoredEvent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, event_data, timestamp FROM clipboard_events ORDER BY timestamp DESC",
        )?;

        let event_iter = stmt.query_map([], |row| {
            let timestamp_str: String = row.get(2)?;
            let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp_str)
                .unwrap()
                .with_timezone(&chrono::Utc);

            Ok(StoredEvent {
                id: row.get(0)?,
                event_data: row.get(1)?,
                timestamp,
            })
        })?;

        let mut events = Vec::new();
        for event in event_iter {
            events.push(event?);
        }
        Ok(events)
    }

    pub fn get_event_by_id(&self, id: &str) -> Result<Option<Event>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT event_data FROM clipboard_events WHERE id = ?1")?;

        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            let event_data: String = row.get(0)?;
            let event: Event = serde_json::from_str(&event_data)
                .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    pub fn delete_event(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM clipboard_events WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_all_events(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM clipboard_events", [])?;
        Ok(())
    }
}
