use crate::event::{deserialize_event, serialize_event, ClipboardEvent};
use chrono::Utc;
use copy_event_listener::event::{Data, Event, Item};
use rusqlite::{types::ValueRef, Connection, Result};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tauri::AppHandle;

const APP_DATA_DIR: &str = ".copy_stack";
const DB_FILE_NAME: &str = "copy_stack.db";
const DEFAULT_MAX_ITEMS: u32 = 100;
const MAX_ITEMS_KEY: &str = "max_items";
const SHOW_IN_MENU_BAR_KEY: &str = "show_in_menu_bar";
const MOVE_RESTORED_ITEM_TO_TOP_KEY: &str = "move_restored_item_to_top";

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredEvent {
    pub content_hash: String,
    pub event_data: String,
    pub timestamp: i64,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AppSettings {
    pub max_items: u32,
    pub show_in_menu_bar: bool,
    pub move_restored_item_to_top: bool,
}

impl StoredEvent {
    fn new(content_hash: String, event_data: String, timestamp: i64) -> Self {
        Self {
            content_hash,
            event_data,
            timestamp,
        }
    }
}

struct DbRow {
    content_hash: Option<String>,
    event_data: String,
    timestamp: i64,
}

struct HashFragment {
    data_type: String,
    value: Vec<u8>,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(_app_handle: &AppHandle) -> Result<Self> {
        let db_path = Self::database_path()?;
        debug_log!("[copy_stack] database path: {}", db_path.display());
        let conn = Connection::open(&db_path)?;
        let db = Self { conn };

        db.initialize_schema()?;

        Ok(db)
    }

    fn database_path() -> Result<PathBuf> {
        let home_dir = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| rusqlite::Error::InvalidPath(PathBuf::from("~")))?;
        let data_dir = home_dir.join(APP_DATA_DIR);
        std::fs::create_dir_all(&data_dir)
            .map_err(|_| rusqlite::Error::InvalidPath(data_dir.clone()))?;
        Ok(data_dir.join(DB_FILE_NAME))
    }

    fn initialize_schema(&self) -> Result<()> {
        self.ensure_clipboard_events_schema()?;

        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        self.conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('max_items', '100')",
            [],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('show_in_menu_bar', 'true')",
            [],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('move_restored_item_to_top', 'false')",
            [],
        )?;

        self.rebuild_history_metadata()?;

        Ok(())
    }

    fn ensure_clipboard_events_schema(&self) -> Result<()> {
        if !self.table_exists("clipboard_events")? {
            self.create_clipboard_events_table("clipboard_events")?;
            self.create_clipboard_events_indexes()?;
            return Ok(());
        }

        let columns = self.table_columns("clipboard_events")?;
        let has_legacy_columns = columns
            .iter()
            .any(|column| column == "id" || column == "sort_order");
        let missing_required_columns = !columns.iter().any(|column| column == "content_hash")
            || !columns.iter().any(|column| column == "event_data")
            || !columns.iter().any(|column| column == "timestamp");

        if has_legacy_columns
            || missing_required_columns
            || !self.primary_key_column_is("clipboard_events", "content_hash")?
        {
            self.rebuild_clipboard_events_table(&columns)?;
        } else {
            self.drop_legacy_clipboard_events_indexes()?;
            self.create_clipboard_events_indexes()?;
        }

        Ok(())
    }

    fn table_exists(&self, table: &str) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
        let mut rows = stmt.query([table])?;
        Ok(rows.next()?.is_some())
    }

    fn table_columns(&self, table: &str) -> Result<Vec<String>> {
        let pragma = format!("PRAGMA table_info({})", table);
        let mut stmt = self.conn.prepare(&pragma)?;
        let mut rows = stmt.query([])?;
        let mut columns = Vec::new();

        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            columns.push(name);
        }

        Ok(columns)
    }

    fn primary_key_column_is(&self, table: &str, expected_column: &str) -> Result<bool> {
        let pragma = format!("PRAGMA table_info({})", table);
        let mut stmt = self.conn.prepare(&pragma)?;
        let mut rows = stmt.query([])?;

        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            let primary_key_position: i64 = row.get(5)?;
            if primary_key_position > 0 {
                return Ok(name == expected_column);
            }
        }

        Ok(false)
    }

    fn create_clipboard_events_table(&self, table: &str) -> Result<()> {
        self.conn.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {} (
                    content_hash TEXT PRIMARY KEY,
                    event_data TEXT NOT NULL,
                    timestamp INTEGER NOT NULL
                )",
                table
            ),
            [],
        )?;
        Ok(())
    }

    fn create_clipboard_events_indexes(&self) -> Result<()> {
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_clipboard_events_timestamp
             ON clipboard_events(timestamp DESC)",
            [],
        )?;
        Ok(())
    }

    fn drop_legacy_clipboard_events_indexes(&self) -> Result<()> {
        self.conn
            .execute("DROP INDEX IF EXISTS idx_clipboard_events_content_hash", [])?;
        self.conn
            .execute("DROP INDEX IF EXISTS idx_clipboard_events_sort_order", [])?;
        Ok(())
    }

    fn rebuild_clipboard_events_table(&self, columns: &[String]) -> Result<()> {
        self.conn
            .execute("DROP TABLE IF EXISTS clipboard_events_next", [])?;
        self.create_clipboard_events_table("clipboard_events_next")?;

        let rows = self.read_clipboard_event_rows(columns)?;
        self.insert_deduped_rows("clipboard_events_next", rows)?;

        self.drop_legacy_clipboard_events_indexes()?;
        self.conn.execute("DROP TABLE clipboard_events", [])?;
        self.conn.execute(
            "ALTER TABLE clipboard_events_next RENAME TO clipboard_events",
            [],
        )?;
        self.create_clipboard_events_indexes()?;

        Ok(())
    }

    fn read_clipboard_event_rows(&self, columns: &[String]) -> Result<Vec<DbRow>> {
        let select_content_hash = if columns.iter().any(|column| column == "content_hash") {
            "content_hash"
        } else {
            "NULL AS content_hash"
        };
        let order_clause = if columns.iter().any(|column| column == "sort_order") {
            "ORDER BY sort_order DESC, timestamp DESC"
        } else {
            "ORDER BY timestamp DESC"
        };
        let query = format!(
            "SELECT {}, event_data, timestamp FROM clipboard_events {}",
            select_content_hash, order_clause
        );

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| {
            Ok(DbRow {
                content_hash: row.get(0)?,
                event_data: row.get(1)?,
                timestamp: Self::timestamp_from_row(row, 2)?,
            })
        })?;

        let mut event_rows = Vec::new();
        for row in rows {
            event_rows.push(row?);
        }

        Ok(event_rows)
    }

    fn timestamp_from_row(row: &rusqlite::Row<'_>, index: usize) -> Result<i64> {
        match row.get_ref(index)? {
            ValueRef::Integer(value) => Ok(Self::normalize_unix_timestamp(value)),
            ValueRef::Real(value) => Ok(Self::normalize_unix_timestamp(value as i64)),
            ValueRef::Text(value) => {
                let text = std::str::from_utf8(value)
                    .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))?;
                Self::parse_timestamp(text)
            }
            ValueRef::Null => Ok(0),
            ValueRef::Blob(_) => Err(rusqlite::Error::InvalidParameterName(
                "timestamp must be text or integer".to_string(),
            )),
        }
    }

    fn parse_timestamp(value: &str) -> Result<i64> {
        if let Ok(timestamp) = value.parse::<i64>() {
            return Ok(Self::normalize_unix_timestamp(timestamp));
        }

        chrono::DateTime::parse_from_rfc3339(value)
            .map(|timestamp| timestamp.timestamp_millis())
            .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))
    }

    fn normalize_unix_timestamp(timestamp: i64) -> i64 {
        if timestamp.abs() < 10_000_000_000 {
            timestamp * 1000
        } else {
            timestamp
        }
    }

    fn rebuild_history_metadata(&self) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "SELECT content_hash, event_data, timestamp
             FROM clipboard_events
             ORDER BY timestamp DESC, content_hash ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DbRow {
                content_hash: row.get(0)?,
                event_data: row.get(1)?,
                timestamp: row.get(2)?,
            })
        })?;

        let mut event_rows = Vec::new();

        for row in rows {
            event_rows.push(row?);
        }
        drop(stmt);

        self.conn
            .execute("DROP TABLE IF EXISTS clipboard_events_next", [])?;
        self.create_clipboard_events_table("clipboard_events_next")?;
        self.insert_deduped_rows("clipboard_events_next", event_rows)?;
        self.conn.execute("DROP TABLE clipboard_events", [])?;
        self.conn.execute(
            "ALTER TABLE clipboard_events_next RENAME TO clipboard_events",
            [],
        )?;
        self.create_clipboard_events_indexes()?;

        Ok(())
    }

    fn insert_deduped_rows(&self, table: &str, rows: Vec<DbRow>) -> Result<()> {
        let mut seen_hashes = std::collections::HashSet::new();

        for row in rows {
            let content_hash = match self.generate_content_hash_from_event_data(&row.event_data) {
                Ok(content_hash) => content_hash,
                Err(_) => row.content_hash.unwrap_or_default(),
            };
            if content_hash.is_empty() || !seen_hashes.insert(content_hash.clone()) {
                continue;
            }

            self.conn.execute(
                &format!(
                    "INSERT INTO {} (content_hash, event_data, timestamp)
                     VALUES (?1, ?2, ?3)",
                    table
                ),
                (content_hash, row.event_data, row.timestamp),
            )?;
        }

        Ok(())
    }

    pub fn get_settings(&self) -> Result<AppSettings> {
        Ok(AppSettings {
            max_items: self.get_max_items()?,
            show_in_menu_bar: self.get_show_in_menu_bar()?,
            move_restored_item_to_top: self.get_move_restored_item_to_top()?,
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
        self.get_bool_setting(SHOW_IN_MENU_BAR_KEY, true)
    }

    pub fn set_show_in_menu_bar(&self, show_in_menu_bar: bool) -> Result<()> {
        self.set_setting(
            SHOW_IN_MENU_BAR_KEY,
            if show_in_menu_bar { "true" } else { "false" },
        )?;
        Ok(())
    }

    pub fn get_move_restored_item_to_top(&self) -> Result<bool> {
        self.get_bool_setting(MOVE_RESTORED_ITEM_TO_TOP_KEY, false)
    }

    pub fn set_move_restored_item_to_top(&self, move_restored_item_to_top: bool) -> Result<()> {
        self.set_setting(
            MOVE_RESTORED_ITEM_TO_TOP_KEY,
            if move_restored_item_to_top {
                "true"
            } else {
                "false"
            },
        )?;
        Ok(())
    }

    pub fn insert_event(&self, event: &Event) -> Result<()> {
        let event_data = serialize_event(event)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
        let content_hash = self.generate_content_hash(event)?;

        let updated = self.conn.execute(
            "UPDATE clipboard_events
             SET event_data = ?1
             WHERE content_hash = ?2",
            (&event_data, &content_hash),
        )?;

        if updated > 0 {
            return Ok(());
        }

        let timestamp = self.next_history_timestamp()?;
        self.conn.execute(
            "INSERT INTO clipboard_events (content_hash, event_data, timestamp)
             VALUES (?1, ?2, ?3)",
            (content_hash, event_data, timestamp),
        )?;

        self.cleanup_old_events()?;

        Ok(())
    }

    pub fn move_event_to_top(&self, content_hash: &str) -> Result<()> {
        let updated = self.conn.execute(
            "UPDATE clipboard_events
             SET timestamp = ?1
             WHERE content_hash = ?2",
            (self.next_history_timestamp()?, content_hash),
        )?;

        if updated == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        Ok(())
    }

    fn next_history_timestamp(&self) -> Result<i64> {
        let max_timestamp: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(timestamp), 0) FROM clipboard_events",
            [],
            |row| row.get(0),
        )?;
        Ok(Self::current_unix_timestamp().max(max_timestamp + 1))
    }

    fn current_unix_timestamp() -> i64 {
        Utc::now().timestamp_millis()
    }

    fn generate_content_hash_from_event_data(&self, event_data: &str) -> Result<String> {
        match deserialize_event(event_data) {
            Ok(event) => self.generate_content_hash(&event),
            Err(_) => {
                let mut hasher = Sha256::new();
                hasher.update(event_data.as_bytes());
                Ok(format!("{:x}", hasher.finalize()))
            }
        }
    }

    fn generate_content_hash(&self, event: &Event) -> Result<String> {
        let fragments = self.extract_hash_fragments(event);
        let mut hasher = Sha256::new();

        if fragments.is_empty() {
            let fallback = serialize_event(event)
                .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
            hasher.update(fallback.as_bytes());
        } else {
            for fragment in fragments {
                hasher.update(fragment.data_type.as_bytes());
                hasher.update([0]);
                hasher.update(fragment.value);
                hasher.update([0xff]);
            }
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    pub fn event_content_hash(&self, event: &Event) -> Result<String> {
        self.generate_content_hash(event)
    }

    fn extract_hash_fragments(&self, event: &Event) -> Vec<HashFragment> {
        let mut fragments = Vec::new();

        for item in &event.items {
            if let Some(fragment) = Self::extract_preferred_fragment(item) {
                fragments.push(fragment);
                continue;
            }

            if let Some(fragment) = Self::extract_fallback_fragment(item) {
                fragments.push(fragment);
            }
        }

        fragments
    }

    fn extract_preferred_fragment(item: &Item) -> Option<HashFragment> {
        const PREFERRED_TYPES: &[&str] = &[
            "public.utf8-plain-text",
            "public.utf16-plain-text",
            "public.plain-text",
            "public.text",
            "text/plain",
            "NSStringPboardType",
            "public.url",
            "public.file-url",
            "text/uri-list",
        ];

        for preferred_type in PREFERRED_TYPES {
            for data in &item.data_list {
                if data.r#type == *preferred_type {
                    if let Some(text) = Self::decode_text(data) {
                        return Some(HashFragment {
                            data_type: data.r#type.clone(),
                            value: Self::normalize_text(&text).into_bytes(),
                        });
                    }
                }
            }
        }

        None
    }

    fn extract_fallback_fragment(item: &Item) -> Option<HashFragment> {
        item.data_list
            .iter()
            .min_by(|left, right| left.r#type.cmp(&right.r#type))
            .map(|data| HashFragment {
                data_type: data.r#type.clone(),
                value: data.data.clone(),
            })
    }

    fn decode_text(data: &Data) -> Option<String> {
        match data.r#type.as_str() {
            "public.utf16-plain-text" => Self::decode_utf16(&data.data),
            _ => Some(String::from_utf8_lossy(&data.data).into_owned()),
        }
    }

    fn decode_utf16(bytes: &[u8]) -> Option<String> {
        if bytes.len() % 2 != 0 {
            return None;
        }

        let (is_big_endian, offset) = match bytes {
            [0xfe, 0xff, ..] => (true, 2),
            [0xff, 0xfe, ..] => (false, 2),
            _ => (false, 0),
        };

        let mut units = Vec::with_capacity((bytes.len() - offset) / 2);
        for chunk in bytes[offset..].chunks_exact(2) {
            let unit = if is_big_endian {
                u16::from_be_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_le_bytes([chunk[0], chunk[1]])
            };
            units.push(unit);
        }

        Some(String::from_utf16_lossy(&units))
    }

    fn normalize_text(input: &str) -> String {
        input
            .chars()
            .filter(|ch| *ch != '\0')
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn get_all_events(&self) -> Result<Vec<StoredEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT content_hash, event_data, timestamp
             FROM clipboard_events
             ORDER BY timestamp DESC, content_hash ASC",
        )?;

        let event_iter = stmt.query_map([], |row| {
            Ok(StoredEvent::new(row.get(0)?, row.get(1)?, row.get(2)?))
        })?;

        let mut events = Vec::new();
        for event in event_iter {
            events.push(event?);
        }
        Ok(events)
    }

    pub fn get_event_by_content_hash(&self, content_hash: &str) -> Result<Option<Event>> {
        let mut stmt = self
            .conn
            .prepare("SELECT event_data FROM clipboard_events WHERE content_hash = ?1")?;

        let mut rows = stmt.query([content_hash])?;
        if let Some(row) = rows.next()? {
            let event_data: String = row.get(0)?;
            let event = deserialize_event(&event_data)
                .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    pub fn get_clipboard_event_by_content_hash(
        &self,
        content_hash: &str,
    ) -> Result<Option<ClipboardEvent>> {
        let mut stmt = self
            .conn
            .prepare("SELECT event_data FROM clipboard_events WHERE content_hash = ?1")?;

        let mut rows = stmt.query([content_hash])?;
        if let Some(row) = rows.next()? {
            let event_data: String = row.get(0)?;
            let event: ClipboardEvent = serde_json::from_str(&event_data)
                .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    pub fn delete_event(&self, content_hash: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM clipboard_events WHERE content_hash = ?1",
            [content_hash],
        )?;
        Ok(())
    }

    pub fn clear_all_events(&self) -> Result<()> {
        self.conn.execute("DELETE FROM clipboard_events", [])?;
        Ok(())
    }

    pub fn cleanup_old_events(&self) -> Result<()> {
        let max_items = self.get_max_items()?;
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM clipboard_events", [], |row| {
                    row.get(0)
                })?;

        if count > max_items as i64 {
            let excess = count - max_items as i64;
            self.conn.execute(
                "DELETE FROM clipboard_events WHERE content_hash IN (
                    SELECT content_hash FROM clipboard_events
                    ORDER BY timestamp ASC, content_hash DESC
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

    fn get_bool_setting(&self, key: &str, default: bool) -> Result<bool> {
        let value = self.get_string_setting(key)?;
        Ok(match value.as_deref() {
            Some("false") => false,
            Some("0") => false,
            Some("true") => true,
            Some("1") => true,
            Some(_) | None => default,
        })
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
