use chrono::{DateTime, Utc};
use copy_event_listener::event::Event;
use rusqlite::{Connection, Result};
use serde_json;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const DEFAULT_MAX_ITEMS: u32 = 100;
const MAX_ITEMS_KEY: &str = "max_items";
const SHOW_IN_MENU_BAR_KEY: &str = "show_in_menu_bar";

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredEvent {
    pub id: String,
    pub event_data: String, // JSON serialized Event
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AppSettings {
    pub max_items: u32,
    pub show_in_menu_bar: bool,
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
    conn: Connection,
}

impl Database {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        let app_data_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|_| rusqlite::Error::InvalidPath(std::path::PathBuf::from("app_data_dir")))?;
        std::fs::create_dir_all(&app_data_dir)
            .map_err(|_| rusqlite::Error::InvalidPath(app_data_dir.clone()))?;

        let db_path = app_data_dir.join("copy_stack.db");
        let conn = Connection::open(db_path)?;

        // Create tables for storing copy_event_listener::event::Event
        conn.execute(
            "CREATE TABLE IF NOT EXISTS clipboard_events (
                id TEXT PRIMARY KEY,
                event_data TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                content_hash TEXT UNIQUE
            )",
            [],
        )?;

        // Create settings table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        // Initialize default max_items if not exists
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('max_items', '100')",
            [],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('show_in_menu_bar', 'true')",
            [],
        )?;

        Ok(Self { conn })
    }

    pub fn get_settings(&self) -> Result<AppSettings> {
        Ok(AppSettings {
            max_items: self.get_max_items()?,
            show_in_menu_bar: self.get_show_in_menu_bar()?,
        })
    }

    pub fn get_max_items(&self) -> Result<u32> {
        self.get_u32_setting(MAX_ITEMS_KEY, DEFAULT_MAX_ITEMS)
    }

    pub fn set_max_items(&self, max_items: u32) -> Result<()> {
        self.set_setting(MAX_ITEMS_KEY, &max_items.to_string())?;
        Ok(())
    }

    pub fn get_show_in_menu_bar(&self) -> Result<bool> {
        let value = self.get_string_setting(SHOW_IN_MENU_BAR_KEY)?;
        Ok(match value.as_deref() {
            Some("false") => false,
            Some("0") => false,
            Some("true") => true,
            Some("1") => true,
            Some(_) | None => true,
        })
    }

    pub fn set_show_in_menu_bar(&self, show_in_menu_bar: bool) -> Result<()> {
        self.set_setting(
            SHOW_IN_MENU_BAR_KEY,
            if show_in_menu_bar { "true" } else { "false" },
        )?;
        Ok(())
    }

    pub fn insert_event(&self, event: &Event) -> Result<()> {
        // Generate content hash for deduplication
        let content_hash = self.generate_content_hash(event)?;

        let stored_event = StoredEvent::new(event.clone())
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

        // Check if content already exists (deduplication)
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM clipboard_events WHERE content_hash = ?1")?;
        let mut rows = stmt.query([&content_hash])?;

        if let Some(row) = rows.next()? {
            // Content already exists, update the timestamp instead of inserting
            let existing_id: String = row.get(0)?;
            self.conn.execute(
                "UPDATE clipboard_events SET timestamp = ?1 WHERE id = ?2",
                [&stored_event.timestamp.to_rfc3339(), &existing_id],
            )?;
            return Ok(());
        }

        // Insert the new event with content hash
        self.conn.execute(
            "INSERT INTO clipboard_events (id, event_data, timestamp, content_hash) VALUES (?1, ?2, ?3, ?4)",
            (
                &stored_event.id,
                &stored_event.event_data,
                &stored_event.timestamp.to_rfc3339(),
                &content_hash,
            ),
        )?;

        // Get current max_items setting
        let max_items = self.get_max_items()?;

        // Count total events
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM clipboard_events", [], |row| {
                    row.get(0)
                })?;

        // If we exceed the limit, delete the oldest events
        if count > max_items as i64 {
            let excess = count - max_items as i64;
            self.conn.execute(
                "DELETE FROM clipboard_events WHERE id IN (
                    SELECT id FROM clipboard_events 
                    ORDER BY timestamp ASC 
                    LIMIT ?1
                )",
                [excess],
            )?;
        }

        Ok(())
    }

    // Helper function to generate content hash for deduplication
    fn generate_content_hash(&self, event: &Event) -> Result<String> {
        // Create a hash from the event content
        let mut hasher = Sha256::new();

        // Hash the serialized event data
        let event_json = serde_json::to_string(event)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
        hasher.update(event_json.as_bytes());

        let result = hasher.finalize();
        Ok(format!("{:x}", result))
    }

    pub fn get_all_events(&self) -> Result<Vec<StoredEvent>> {
        let mut stmt = self.conn.prepare(
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
        let mut stmt = self
            .conn
            .prepare("SELECT event_data FROM clipboard_events WHERE id = ?1")?;

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
        self.conn
            .execute("DELETE FROM clipboard_events WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_all_events(&self) -> Result<()> {
        self.conn.execute("DELETE FROM clipboard_events", [])?;
        Ok(())
    }

    pub fn cleanup_old_events(&self) -> Result<()> {
        let max_items = self.get_max_items()?;

        // Count total events
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM clipboard_events", [], |row| {
                    row.get(0)
                })?;

        // If we exceed the limit, delete the oldest events
        if count > max_items as i64 {
            let excess = count - max_items as i64;
            self.conn.execute(
                "DELETE FROM clipboard_events WHERE id IN (
                    SELECT id FROM clipboard_events 
                    ORDER BY timestamp ASC 
                    LIMIT ?1
                )",
                [excess],
            )?;
        }

        Ok(())
    }

    fn get_u32_setting(&self, key: &str, default: u32) -> Result<u32> {
        match self.get_string_setting(key)? {
            Some(value) => value
                .parse::<u32>()
                .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string())),
            None => Ok(default),
        }
    }

    fn get_string_setting(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query([key])?;

        if let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            [key, value],
        )?;
        Ok(())
    }
}
