use crate::event::{decode_event_blob, encode_event_blob, event_from_legacy_json, ClipboardEvent};
use chrono::Utc;
use copy_event_listener::event::{Data, Event, Item};
use rusqlite::{types::ValueRef, Connection, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use tauri::AppHandle;

const APP_DATA_DIR: &str = ".copy_stack";
const DB_FILE_NAME: &str = "copy_stack.db";
const DEFAULT_MAX_ITEMS: u32 = 100;
const MAX_ITEMS_KEY: &str = "max_items";
const SHOW_IN_MENU_BAR_KEY: &str = "show_in_menu_bar";
const MOVE_RESTORED_ITEM_TO_TOP_KEY: &str = "move_restored_item_to_top";
const FILE_DISPLAY_FORMAT: &str = "copy_stack.file-items.v1";
const INLINE_ATTACHMENT_PLACEHOLDER: char = '\u{fffc}';

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredEvent {
    pub content_hash: String,
    pub data_type: String,
    pub display: Vec<u8>,
    pub rich_preview: Vec<StoredPreviewSegment>,
    pub timestamp: i64,
    pub source_app: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum StoredPreviewSegment {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        label: String,
        media_type: String,
        data: Vec<u8>,
    },
    #[serde(rename = "video")]
    Video {
        label: String,
        media_type: String,
        path: String,
    },
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AppSettings {
    pub max_items: u32,
    pub show_in_menu_bar: bool,
    pub move_restored_item_to_top: bool,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FileDisplay {
    pub format: String,
    pub items: Vec<FileDisplayItem>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FileDisplayItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub name: String,
}

impl StoredEvent {
    fn new(
        content_hash: String,
        data_type: String,
        display: Vec<u8>,
        rich_preview: Vec<StoredPreviewSegment>,
        timestamp: i64,
        source_app: Option<String>,
    ) -> Self {
        Self {
            content_hash,
            data_type,
            display,
            rich_preview,
            timestamp,
            source_app,
        }
    }
}

struct DbRow {
    event_data: Vec<u8>,
    timestamp: i64,
    source_app: Option<String>,
}

struct ClassifiedEvent {
    content_hash: String,
    data_type: String,
    display: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct HistoryJsonlConfig {
    pub path: PathBuf,
    pub max_data_bytes: usize,
}

#[derive(Serialize)]
struct HistoryJsonlRecord {
    content_hash: String,
    data_type: String,
    timestamp: i64,
    display: HistoryJsonlBytes,
    event_data: HistoryJsonlEvent,
}

#[derive(Serialize)]
struct HistoryJsonlEvent {
    items: Vec<HistoryJsonlItem>,
}

#[derive(Serialize)]
struct HistoryJsonlItem {
    data_list: Vec<HistoryJsonlData>,
}

#[derive(Serialize)]
struct HistoryJsonlData {
    #[serde(rename = "type")]
    data_type: String,
    data: HistoryJsonlBytes,
}

#[derive(Serialize)]
struct HistoryJsonlBytes {
    byte_len: usize,
    truncated: bool,
    encoding: &'static str,
    value: String,
}

pub struct Database {
    conn: Connection,
}

impl HistoryJsonlBytes {
    fn new(bytes: &[u8], max_data_bytes: usize) -> Self {
        let byte_len = bytes.len();
        let truncated = byte_len > max_data_bytes;

        if let Some(value) = Self::utf8_value(bytes, max_data_bytes) {
            return Self {
                byte_len,
                truncated,
                encoding: "utf8",
                value,
            };
        }

        let visible_len = byte_len.min(max_data_bytes);
        Self {
            byte_len,
            truncated,
            encoding: "hex",
            value: hex_bytes(&bytes[..visible_len]),
        }
    }

    fn utf8_value(bytes: &[u8], max_data_bytes: usize) -> Option<String> {
        let text = std::str::from_utf8(bytes).ok()?;
        if bytes.len() <= max_data_bytes {
            return Some(text.to_string());
        }

        let mut end = max_data_bytes;
        while !text.is_char_boundary(end) {
            end = end.checked_sub(1)?;
        }

        Some(text[..end].to_string())
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }

    output
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
            || !columns.iter().any(|column| column == "data_type")
            || !columns.iter().any(|column| column == "display")
            || !columns.iter().any(|column| column == "timestamp");
        let event_data_is_blob = self
            .column_declared_type("clipboard_events", "event_data")?
            .is_some_and(|column_type| column_type.eq_ignore_ascii_case("BLOB"));
        let display_is_blob = self
            .column_declared_type("clipboard_events", "display")?
            .is_some_and(|column_type| column_type.eq_ignore_ascii_case("BLOB"));

        if has_legacy_columns
            || missing_required_columns
            || !event_data_is_blob
            || !display_is_blob
            || !self.primary_key_column_is("clipboard_events", "content_hash")?
        {
            self.rebuild_clipboard_events_table(&columns)?;
        } else {
            if !columns.iter().any(|column| column == "source_app") {
                self.conn.execute(
                    "ALTER TABLE clipboard_events ADD COLUMN source_app TEXT",
                    [],
                )?;
            }
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

    fn column_declared_type(&self, table: &str, expected_column: &str) -> Result<Option<String>> {
        let pragma = format!("PRAGMA table_info({})", table);
        let mut stmt = self.conn.prepare(&pragma)?;
        let mut rows = stmt.query([])?;

        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == expected_column {
                return Ok(Some(row.get(2)?));
            }
        }

        Ok(None)
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
                    event_data BLOB NOT NULL,
                    data_type TEXT NOT NULL,
                    display BLOB NOT NULL,
                    timestamp INTEGER NOT NULL,
                    source_app TEXT
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
        let order_clause = if columns.iter().any(|column| column == "sort_order") {
            "ORDER BY sort_order DESC, timestamp DESC"
        } else {
            "ORDER BY timestamp DESC"
        };
        let source_app_select = if columns.iter().any(|column| column == "source_app") {
            "source_app"
        } else {
            "NULL AS source_app"
        };
        let query = format!(
            "SELECT event_data, timestamp, {} FROM clipboard_events {}",
            source_app_select, order_clause
        );

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| {
            Ok(DbRow {
                event_data: Self::event_blob_from_row(row, 0)?,
                timestamp: Self::timestamp_from_row(row, 1)?,
                source_app: Self::normalized_source_app(row.get(2)?),
            })
        })?;

        let mut event_rows = Vec::new();
        for row in rows {
            event_rows.push(row?);
        }

        Ok(event_rows)
    }

    fn event_blob_from_row(row: &rusqlite::Row<'_>, index: usize) -> Result<Vec<u8>> {
        match row.get_ref(index)? {
            ValueRef::Blob(value) => Ok(value.to_vec()),
            ValueRef::Text(value) => {
                let text = std::str::from_utf8(value)
                    .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))?;
                let event = event_from_legacy_json(text)
                    .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))?;
                encode_event_blob(&event)
                    .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))
            }
            ValueRef::Null => Err(rusqlite::Error::InvalidParameterName(
                "event_data cannot be null".to_string(),
            )),
            ValueRef::Integer(_) | ValueRef::Real(_) => Err(rusqlite::Error::InvalidParameterName(
                "event_data must be text or blob".to_string(),
            )),
        }
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
            "SELECT event_data, timestamp, source_app
             FROM clipboard_events
             ORDER BY timestamp DESC, content_hash ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DbRow {
                event_data: Self::event_blob_from_row(row, 0)?,
                timestamp: row.get(1)?,
                source_app: Self::normalized_source_app(row.get(2)?),
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
            let classified = match Self::classify_event_from_event_data(&row.event_data) {
                Ok(classified) => classified,
                Err(_) => continue,
            };
            if classified.content_hash.is_empty()
                || !seen_hashes.insert(classified.content_hash.clone())
            {
                continue;
            }

            self.conn.execute(
                &format!(
                    "INSERT INTO {} (content_hash, event_data, data_type, display, timestamp, source_app)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    table
                ),
                (
                    classified.content_hash,
                    row.event_data,
                    classified.data_type,
                    classified.display,
                    row.timestamp,
                    row.source_app,
                ),
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

    pub fn insert_event(&self, event: &Event, source_app: Option<String>) -> Result<()> {
        let event_data = encode_event_blob(event)
            .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))?;
        let classified = Self::classify_event(event)?;
        let source_app = Self::normalized_source_app(source_app);

        let updated = self.conn.execute(
            "UPDATE clipboard_events
             SET event_data = ?1, data_type = ?2, display = ?3, source_app = COALESCE(?4, source_app)
             WHERE content_hash = ?5",
            (
                &event_data,
                &classified.data_type,
                &classified.display,
                &source_app,
                &classified.content_hash,
            ),
        )?;

        if updated > 0 {
            return Ok(());
        }

        let timestamp = self.next_history_timestamp()?;
        self.conn.execute(
            "INSERT INTO clipboard_events (content_hash, event_data, data_type, display, timestamp, source_app)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                classified.content_hash,
                event_data,
                classified.data_type,
                classified.display,
                timestamp,
                source_app,
            ),
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

    pub fn event_content_hash(&self, event: &Event) -> Result<String> {
        Ok(Self::classify_event(event)?.content_hash)
    }

    fn classify_event_from_event_data(event_data: &[u8]) -> Result<ClassifiedEvent> {
        let event = decode_event_blob(event_data)
            .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))?;
        Self::classify_event(&event)
    }

    fn classify_event(event: &Event) -> Result<ClassifiedEvent> {
        if let Some(classified) = Self::classify_special_event(event) {
            return Ok(classified);
        }

        Self::classify_unsupported_event(event)
    }

    fn classify_unsupported_event(event: &Event) -> Result<ClassifiedEvent> {
        let event_data = encode_event_blob(event)
            .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))?;

        Ok(ClassifiedEvent {
            content_hash: Self::hash_bytes(&event_data),
            data_type: "unsupported".to_string(),
            display: Self::display_bytes(Self::unsupported_event_display(event)),
        })
    }

    fn unsupported_event_display(event: &Event) -> String {
        let mut data_types = Vec::new();

        for item in &event.items {
            for data in &item.data_list {
                if !data_types.contains(&data.r#type) {
                    data_types.push(data.r#type.clone());
                }
            }
        }

        if data_types.is_empty() {
            return "Unsupported clipboard data".to_string();
        }

        let visible = data_types.iter().take(3).cloned().collect::<Vec<_>>();
        let suffix = data_types
            .len()
            .checked_sub(visible.len())
            .filter(|hidden| *hidden > 0)
            .map(|hidden| format!(" + {} more", hidden))
            .unwrap_or_default();

        format!(
            "Unsupported clipboard data: {}{}",
            visible.join(", "),
            suffix
        )
    }

    pub fn parse_file_display(display: &[u8]) -> Option<FileDisplay> {
        serde_json::from_slice::<FileDisplay>(display)
            .ok()
            .filter(|display| display.format == FILE_DISPLAY_FORMAT)
    }

    pub fn write_history_jsonl(
        &self,
        config: &HistoryJsonlConfig,
    ) -> std::result::Result<(), String> {
        if let Some(parent) = config.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("failed to create JSONL directory: {}", error))?;
            }
        }

        let file = File::create(&config.path)
            .map_err(|error| format!("failed to create JSONL file: {}", error))?;
        let mut writer = BufWriter::new(file);

        let mut stmt = self
            .conn
            .prepare(
                "SELECT content_hash, event_data, data_type, display, timestamp
                 FROM clipboard_events
                 ORDER BY timestamp DESC, content_hash ASC",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = stmt.query([]).map_err(|error| error.to_string())?;

        while let Some(row) = rows.next().map_err(|error| error.to_string())? {
            let content_hash: String = row.get(0).map_err(|error| error.to_string())?;
            let event_data: Vec<u8> = row.get(1).map_err(|error| error.to_string())?;
            let data_type: String = row.get(2).map_err(|error| error.to_string())?;
            let display: Vec<u8> = row.get(3).map_err(|error| error.to_string())?;
            let timestamp: i64 = row.get(4).map_err(|error| error.to_string())?;
            let event = Self::event_from_blob(&event_data).map_err(|error| error.to_string())?;
            let record = Self::history_jsonl_record(
                content_hash,
                data_type,
                timestamp,
                &display,
                &event,
                config.max_data_bytes,
            );

            serde_json::to_writer(&mut writer, &record)
                .map_err(|error| format!("failed to write JSONL row: {}", error))?;
            writer
                .write_all(b"\n")
                .map_err(|error| format!("failed to write JSONL newline: {}", error))?;
        }

        writer
            .flush()
            .map_err(|error| format!("failed to flush JSONL file: {}", error))
    }

    fn history_jsonl_record(
        content_hash: String,
        data_type: String,
        timestamp: i64,
        display: &[u8],
        event: &Event,
        max_data_bytes: usize,
    ) -> HistoryJsonlRecord {
        HistoryJsonlRecord {
            content_hash,
            data_type,
            timestamp,
            display: HistoryJsonlBytes::new(display, max_data_bytes),
            event_data: HistoryJsonlEvent {
                items: event
                    .items
                    .iter()
                    .map(|item| HistoryJsonlItem {
                        data_list: item
                            .data_list
                            .iter()
                            .map(|data| HistoryJsonlData {
                                data_type: data.r#type.clone(),
                                data: HistoryJsonlBytes::new(&data.data, max_data_bytes),
                            })
                            .collect(),
                    })
                    .collect(),
            },
        }
    }

    fn rich_preview_from_event_data(event_data: &[u8]) -> Vec<StoredPreviewSegment> {
        let Ok(event) = Self::event_from_blob(event_data) else {
            return Vec::new();
        };
        let rich_preview = Self::rich_preview_segments(&event);
        if rich_preview.is_empty() {
            Self::video_preview_segments(&event)
        } else {
            rich_preview
        }
    }

    fn rich_preview_segments(event: &Event) -> Vec<StoredPreviewSegment> {
        let Some(text_template) = Self::find_raw_utf8_display(event) else {
            return Vec::new();
        };
        if !text_template.contains(INLINE_ATTACHMENT_PLACEHOLDER) {
            return Vec::new();
        }

        let images = Self::rich_preview_images(event);
        if images.is_empty() {
            return Vec::new();
        }

        let mut segments = Vec::new();
        let mut text_buffer = String::new();
        let mut images = images.into_iter();

        for character in text_template.chars() {
            if character == INLINE_ATTACHMENT_PLACEHOLDER {
                Self::push_rich_preview_text(&mut segments, &mut text_buffer);
                if let Some(image) = images.next() {
                    segments.push(image);
                }
            } else {
                text_buffer.push(character);
            }
        }

        Self::push_rich_preview_text(&mut segments, &mut text_buffer);
        segments
    }

    fn push_rich_preview_text(segments: &mut Vec<StoredPreviewSegment>, text: &mut String) {
        let cleaned = text
            .chars()
            .filter(|character| *character != '\0')
            .collect::<String>()
            .trim()
            .to_string();
        text.clear();

        if !cleaned.is_empty() {
            segments.push(StoredPreviewSegment::Text { text: cleaned });
        }
    }

    fn rich_preview_images(event: &Event) -> Vec<StoredPreviewSegment> {
        event
            .items
            .iter()
            .filter_map(Self::rich_preview_image_in_item)
            .collect()
    }

    fn rich_preview_image_in_item(item: &Item) -> Option<StoredPreviewSegment> {
        if let Some(data) = Self::find_data_in_item(item, "public.png") {
            return Some(StoredPreviewSegment::Image {
                label: "Image".to_string(),
                media_type: "image/png".to_string(),
                data: data.data.clone(),
            });
        }

        let file_url = Self::find_data_in_item(item, "public.file-url")?;
        let file_url = String::from_utf8_lossy(&file_url.data);
        let extension = Self::file_url_extension(&file_url)?;
        let media_type = Self::preview_image_media_type(&extension)?;
        let path = Self::file_url_path(&file_url)?;
        let image_data = std::fs::read(path).ok()?;
        let label = Self::file_url_display_name(&file_url).unwrap_or_else(|| "Image".to_string());

        Some(StoredPreviewSegment::Image {
            label,
            media_type: media_type.to_string(),
            data: image_data,
        })
    }

    fn preview_image_media_type(extension: &str) -> Option<&'static str> {
        match extension {
            "png" => Some("image/png"),
            "jpg" | "jpeg" => Some("image/jpeg"),
            "gif" => Some("image/gif"),
            "webp" => Some("image/webp"),
            "bmp" => Some("image/bmp"),
            _ => None,
        }
    }

    fn video_preview_segments(event: &Event) -> Vec<StoredPreviewSegment> {
        if event.items.len() != 1 {
            return Vec::new();
        }

        Self::video_preview_in_item(&event.items[0])
            .into_iter()
            .collect()
    }

    fn video_preview_in_item(item: &Item) -> Option<StoredPreviewSegment> {
        let file_url = Self::find_data_in_item(item, "public.file-url")?;
        let file_url = String::from_utf8_lossy(&file_url.data);
        let extension = Self::file_url_extension(&file_url)?;
        let media_type = Self::preview_video_media_type(&extension)?;
        let path = Self::file_url_path(&file_url)?;
        if !path.is_file() {
            return None;
        }

        let label = Self::file_url_display_name(&file_url).unwrap_or_else(|| "Video".to_string());

        Some(StoredPreviewSegment::Video {
            label,
            media_type: media_type.to_string(),
            path: path.to_string_lossy().into_owned(),
        })
    }

    fn preview_video_media_type(extension: &str) -> Option<&'static str> {
        match extension {
            "mov" => Some("video/quicktime"),
            "mp4" | "m4v" => Some("video/mp4"),
            "webm" => Some("video/webm"),
            "mpeg" | "mpg" => Some("video/mpeg"),
            _ => None,
        }
    }

    fn file_url_path(file_url: &str) -> Option<PathBuf> {
        let path = file_url
            .split(['?', '#'])
            .next()
            .unwrap_or(file_url)
            .strip_prefix("file://")?;
        Some(PathBuf::from(Self::percent_decode(path)))
    }

    fn percent_decode(value: &str) -> String {
        let bytes = value.as_bytes();
        let mut output = Vec::with_capacity(bytes.len());
        let mut index = 0;

        while index < bytes.len() {
            if bytes[index] == b'%' && index + 2 < bytes.len() {
                if let (Some(high), Some(low)) = (
                    Self::hex_digit_value(bytes[index + 1]),
                    Self::hex_digit_value(bytes[index + 2]),
                ) {
                    output.push((high << 4) | low);
                    index += 3;
                    continue;
                }
            }

            output.push(bytes[index]);
            index += 1;
        }

        String::from_utf8_lossy(&output).into_owned()
    }

    fn hex_digit_value(value: u8) -> Option<u8> {
        match value {
            b'0'..=b'9' => Some(value - b'0'),
            b'a'..=b'f' => Some(value - b'a' + 10),
            b'A'..=b'F' => Some(value - b'A' + 10),
            _ => None,
        }
    }

    fn classify_special_event(event: &Event) -> Option<ClassifiedEvent> {
        if let Some(data) = Self::find_data(event, "public.rtf") {
            return Some(Self::classified_from_single_data(
                "rtf",
                &data.data,
                Self::display_bytes(
                    Self::find_utf8_display(event).unwrap_or_else(|| "RTF".to_string()),
                ),
            ));
        }

        if let Some(data) = Self::find_data(event, "public.png") {
            return Some(Self::classified_from_single_data(
                "png",
                &data.data,
                data.data.clone(),
            ));
        }

        if let Some(data) = Self::find_data(event, "public.html") {
            return Some(Self::classified_from_single_data(
                "html",
                &data.data,
                Self::display_bytes(
                    Self::find_utf8_display(event).unwrap_or_else(|| "HTML".to_string()),
                ),
            ));
        }

        if event.items.len() > 1 {
            if let Some(file_urls) = Self::extract_multi_file_urls(event) {
                let data_type = Self::multi_file_url_data_type(&file_urls);
                let display_names = Self::file_display_names(event);
                let display_items = event
                    .items
                    .iter()
                    .enumerate()
                    .filter_map(|(index, item)| {
                        Self::file_display_item(item, index, display_names.get(index).cloned())
                    })
                    .collect::<Vec<_>>();
                let mut hasher = Sha256::new();
                for file_url in &file_urls {
                    hasher.update(file_url);
                }

                return Some(ClassifiedEvent {
                    content_hash: format!("{:x}", hasher.finalize()),
                    data_type: data_type.to_string(),
                    display: Self::file_display_bytes(display_items),
                });
            }
        }

        if event.items.len() == 1 {
            if let Some(data) = Self::find_data_in_item(&event.items[0], "public.file-url") {
                if Self::is_video_file_url(data) {
                    let file_url = String::from_utf8_lossy(&data.data);
                    return Some(Self::classified_from_single_data(
                        "video",
                        &data.data,
                        Self::display_bytes(
                            Self::file_url_display_name(&file_url)
                                .unwrap_or_else(|| "Video".to_string()),
                        ),
                    ));
                }

                if let Some(image_type) = Self::image_file_url_type(&event.items[0], data) {
                    return Some(Self::classified_from_single_data(
                        &image_type,
                        &data.data,
                        image_type.to_uppercase().into_bytes(),
                    ));
                }

                let file_url = String::from_utf8_lossy(&data.data);
                let data_type = if file_url.ends_with('/') {
                    "folder"
                } else {
                    "file"
                };
                let display_name = Self::file_display_names(event).into_iter().next();

                return Some(Self::classified_from_single_data(
                    data_type,
                    &data.data,
                    Self::file_display_bytes(vec![Self::file_display_item_for_url(
                        data_type,
                        &file_url,
                        0,
                        display_name,
                    )]),
                ));
            }
        }

        Self::classify_plain_utf8_text(event)
    }

    fn classify_plain_utf8_text(event: &Event) -> Option<ClassifiedEvent> {
        if event.items.len() != 1 {
            return None;
        }

        let data = Self::find_data_in_item(&event.items[0], "public.utf8-plain-text")?;

        Some(ClassifiedEvent {
            content_hash: Self::hash_bytes(&data.data),
            data_type: "text".to_string(),
            display: data.data.clone(),
        })
    }

    fn image_file_url_type(item: &Item, file_url_data: &Data) -> Option<String> {
        let file_url = String::from_utf8_lossy(&file_url_data.data);
        if file_url.ends_with('/') {
            return None;
        }

        let extension = Self::file_url_extension(&file_url);
        if extension
            .as_deref()
            .is_some_and(Self::is_supported_image_extension)
        {
            return extension;
        }

        if Self::find_data_in_item(item, "public.tiff").is_some_and(|data| !data.data.is_empty()) {
            return Some("tiff".to_string());
        }

        None
    }

    fn is_video_file_url(file_url_data: &Data) -> bool {
        let file_url = String::from_utf8_lossy(&file_url_data.data);
        Self::file_url_extension(&file_url)
            .as_deref()
            .is_some_and(Self::is_supported_video_extension)
    }

    fn file_url_extension(file_url: &str) -> Option<String> {
        let path = file_url
            .split(['?', '#'])
            .next()
            .unwrap_or(file_url)
            .trim_end_matches('/');
        let file_name = path.rsplit('/').next()?;
        let (_, extension) = file_name.rsplit_once('.')?;
        if extension.is_empty() {
            None
        } else {
            Some(extension.to_ascii_lowercase())
        }
    }

    fn is_supported_image_extension(extension: &str) -> bool {
        matches!(
            extension,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "tiff" | "tif" | "bmp" | "heic" | "heif"
        )
    }

    fn is_supported_video_extension(extension: &str) -> bool {
        matches!(
            extension,
            "mov" | "mp4" | "m4v" | "avi" | "webm" | "mkv" | "mpeg" | "mpg"
        )
    }

    fn extract_multi_file_urls(event: &Event) -> Option<Vec<&[u8]>> {
        let mut file_urls = Vec::with_capacity(event.items.len());

        for item in &event.items {
            let file_url = Self::find_data_in_item(item, "public.file-url")?;
            file_urls.push(file_url.data.as_slice());
        }

        Some(file_urls)
    }

    fn multi_file_url_data_type(file_urls: &[&[u8]]) -> &'static str {
        let folder_count = file_urls
            .iter()
            .filter(|file_url| String::from_utf8_lossy(file_url).ends_with('/'))
            .count();

        if folder_count == 0 {
            "files"
        } else if folder_count == file_urls.len() {
            "folders"
        } else {
            "files and folders"
        }
    }

    fn file_display_bytes(items: Vec<FileDisplayItem>) -> Vec<u8> {
        serde_json::to_vec(&FileDisplay {
            format: FILE_DISPLAY_FORMAT.to_string(),
            items,
        })
        .unwrap_or_else(|_| Self::label_for_data_type("files").into_bytes())
    }

    fn file_display_item(
        item: &Item,
        index: usize,
        display_name: Option<String>,
    ) -> Option<FileDisplayItem> {
        let file_url = Self::find_data_in_item(item, "public.file-url")?;
        let file_url = String::from_utf8_lossy(&file_url.data);
        let item_type = if file_url.ends_with('/') {
            "folder"
        } else {
            "file"
        };

        Some(Self::file_display_item_for_url(
            item_type,
            &file_url,
            index,
            display_name.or_else(|| Self::file_display_name_in_item(item)),
        ))
    }

    fn file_display_item_for_url(
        item_type: &str,
        file_url: &str,
        index: usize,
        display_name: Option<String>,
    ) -> FileDisplayItem {
        let name = display_name
            .or_else(|| {
                Self::file_url_display_name(file_url)
                    .filter(|name| !Self::is_file_reference_display_name(name))
            })
            .unwrap_or_else(|| Self::generic_file_display_name(item_type, index));

        FileDisplayItem {
            item_type: item_type.to_string(),
            name,
        }
    }

    fn file_display_names(event: &Event) -> Vec<String> {
        event
            .items
            .iter()
            .find_map(Self::find_raw_utf8_display_in_item)
            .map(|display| Self::split_file_display_names(&display))
            .unwrap_or_default()
    }

    fn file_display_name_in_item(item: &Item) -> Option<String> {
        Self::find_raw_utf8_display_in_item(item)
            .and_then(|display| Self::split_file_display_names(&display).into_iter().next())
    }

    fn split_file_display_names(display: &str) -> Vec<String> {
        display
            .split('\r')
            .filter_map(Self::safe_text_file_display_name)
            .collect()
    }

    fn file_url_display_name(file_url: &str) -> Option<String> {
        let path = file_url
            .split(['?', '#'])
            .next()
            .unwrap_or(file_url)
            .trim_end_matches('/');
        let path = path.strip_prefix("file://").unwrap_or(path);
        let path = Self::percent_decode(path);
        Self::path_display_name(&path)
    }

    fn safe_text_file_display_name(display: &str) -> Option<String> {
        let display = display
            .trim_matches(|ch: char| ch == '\0' || ch.is_whitespace())
            .to_string();
        if display.is_empty() || Self::is_aggregate_file_label(&display) {
            return None;
        }

        let display_name = Self::path_display_name(&display).unwrap_or(display);
        if Self::is_file_reference_display_name(&display_name) {
            None
        } else {
            Some(display_name)
        }
    }

    fn is_aggregate_file_label(display: &str) -> bool {
        let normalized = display.split_whitespace().collect::<Vec<_>>().join(" ");
        let Some((count, kind)) = normalized.split_once(' ') else {
            return false;
        };

        count.parse::<usize>().is_ok()
            && matches!(
                kind.to_ascii_lowercase().as_str(),
                "file" | "files" | "folder" | "folders" | "item" | "items"
            )
    }

    fn is_file_reference_display_name(display_name: &str) -> bool {
        display_name == ".file" || display_name.starts_with("id=")
    }

    fn generic_file_display_name(item_type: &str, index: usize) -> String {
        let label = if item_type == "folder" {
            "Folder"
        } else {
            "File"
        };
        format!("{} {}", label, index + 1)
    }

    fn path_display_name(path: &str) -> Option<String> {
        path.rsplit('/')
            .find(|part| !part.is_empty())
            .map(str::to_string)
    }

    fn label_for_data_type(data_type: &str) -> String {
        match data_type {
            "files" => "Files".to_string(),
            "folders" => "Folders".to_string(),
            "files and folders" => "Files and folders".to_string(),
            _ => data_type.to_string(),
        }
    }

    fn display_bytes(display: String) -> Vec<u8> {
        Self::normalize_text(&display).into_bytes()
    }

    fn classified_from_single_data(
        data_type: &str,
        hash_value: &[u8],
        display: Vec<u8>,
    ) -> ClassifiedEvent {
        ClassifiedEvent {
            content_hash: Self::hash_bytes(hash_value),
            data_type: data_type.to_string(),
            display,
        }
    }

    fn hash_bytes(value: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value);
        format!("{:x}", hasher.finalize())
    }

    fn find_data<'event>(event: &'event Event, data_type: &str) -> Option<&'event Data> {
        event
            .items
            .iter()
            .find_map(|item| Self::find_data_in_item(item, data_type))
    }

    fn find_data_in_item<'item>(item: &'item Item, data_type: &str) -> Option<&'item Data> {
        item.data_list.iter().find(|data| data.r#type == data_type)
    }

    fn find_utf8_display(event: &Event) -> Option<String> {
        event.items.iter().find_map(Self::find_utf8_display_in_item)
    }

    fn find_raw_utf8_display(event: &Event) -> Option<String> {
        event
            .items
            .iter()
            .find_map(Self::find_raw_utf8_display_in_item)
    }

    fn find_raw_utf8_display_in_item(item: &Item) -> Option<String> {
        Self::find_data_in_item(item, "public.utf8-plain-text")
            .map(|data| String::from_utf8_lossy(&data.data).into_owned())
    }

    fn find_utf8_display_in_item(item: &Item) -> Option<String> {
        Self::find_data_in_item(item, "public.utf8-plain-text")
            .map(|data| String::from_utf8_lossy(&data.data).into_owned())
            .map(|text| Self::normalize_text(&text))
            .filter(|text| !text.is_empty())
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

    fn normalized_source_app(source_app: Option<String>) -> Option<String> {
        source_app
            .map(|source| Self::normalize_text(&source))
            .filter(|source| !source.is_empty())
    }

    pub fn get_all_events(&self) -> Result<Vec<StoredEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT content_hash, data_type, display, event_data, timestamp, source_app
             FROM clipboard_events
             ORDER BY timestamp DESC, content_hash ASC",
        )?;

        let event_iter = stmt.query_map([], |row| {
            let event_data: Vec<u8> = row.get(3)?;
            Ok(StoredEvent::new(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                Self::rich_preview_from_event_data(&event_data),
                row.get(4)?,
                row.get(5)?,
            ))
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
            let event_data: Vec<u8> = row.get(0)?;
            let event = Self::event_from_blob(&event_data)?;
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
            let event_data: Vec<u8> = row.get(0)?;
            Ok(Some(Self::clipboard_event_from_blob(event_data)?))
        } else {
            Ok(None)
        }
    }

    fn event_from_blob(event_data: &[u8]) -> Result<Event> {
        decode_event_blob(event_data)
            .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))
    }

    fn clipboard_event_from_blob(event_data: Vec<u8>) -> Result<ClipboardEvent> {
        Self::event_from_blob(&event_data).map(|event| ClipboardEvent::from(&event))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn data(data_type: &str, value: &[u8]) -> Data {
        Data {
            r#type: data_type.to_string(),
            data: value.to_vec(),
        }
    }

    fn event(data_list: Vec<Data>) -> Event {
        Event {
            items: vec![Item { data_list }],
        }
    }

    fn in_memory_database() -> Database {
        let db = Database {
            conn: Connection::open_in_memory().expect("in-memory database should open"),
        };
        db.initialize_schema()
            .expect("in-memory schema should initialize");
        db
    }

    fn temp_jsonl_path() -> PathBuf {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "copy_stack_history_{}_{}.jsonl",
            std::process::id(),
            now
        ))
    }

    fn temp_png_path() -> PathBuf {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "copy_stack_preview_{}_{}.png",
            std::process::id(),
            now
        ))
    }

    fn temp_video_path() -> PathBuf {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "copy_stack_preview_{}_{}.mov",
            std::process::id(),
            now
        ))
    }

    fn display_string(classified: &ClassifiedEvent) -> String {
        String::from_utf8_lossy(&classified.display).into_owned()
    }

    fn display_file_items(classified: &ClassifiedEvent) -> Vec<FileDisplayItem> {
        Database::parse_file_display(&classified.display)
            .expect("display should be a file display payload")
            .items
    }

    fn assert_png_display(classified: &ClassifiedEvent, expected: &[u8]) {
        assert_eq!(classified.data_type, "png");
        assert_eq!(classified.display, expected);
    }

    #[test]
    fn classification_prefers_rtf_hash_and_utf8_display() {
        let event = event(vec![
            data("public.utf8-plain-text", b"Visible text"),
            data("public.rtf", b"{\\rtf1 Visible text}"),
        ]);

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "rtf");
        assert_eq!(display_string(&classified), "Visible text");
        assert_eq!(
            classified.content_hash,
            Database::hash_bytes(b"{\\rtf1 Visible text}")
        );
    }

    #[test]
    fn classification_prefers_png_over_html_for_chrome_image_copy() {
        let event = event(vec![
            data("public.png", &[0, 1, 2]),
            data(
                "public.html",
                br#"<meta charset='utf-8'><img src="https://example.test/avatar.avif"/>"#,
            ),
        ]);

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_png_display(&classified, &[0, 1, 2]);
        assert_eq!(classified.content_hash, Database::hash_bytes(&[0, 1, 2]));
    }

    #[test]
    fn classification_marks_single_file_url_folder() {
        let event = event(vec![
            data("public.utf8-plain-text", b"/Users/example/Documents"),
            data("public.file-url", b"file:///Users/example/Documents/"),
        ]);

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "folder");
        assert_eq!(
            display_file_items(&classified),
            vec![FileDisplayItem {
                item_type: "folder".to_string(),
                name: "Documents".to_string(),
            }]
        );
        assert_eq!(
            classified.content_hash,
            Database::hash_bytes(b"file:///Users/example/Documents/")
        );
    }

    #[test]
    fn classification_marks_single_file_url_file_with_basename() {
        let event = event(vec![
            data(
                "public.utf8-plain-text",
                b"/Users/example/Documents/report.pdf",
            ),
            data(
                "public.file-url",
                b"file:///Users/example/Documents/report.pdf",
            ),
        ]);

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "file");
        assert_eq!(
            display_file_items(&classified),
            vec![FileDisplayItem {
                item_type: "file".to_string(),
                name: "report.pdf".to_string(),
            }]
        );
        assert_eq!(
            classified.content_hash,
            Database::hash_bytes(b"file:///Users/example/Documents/report.pdf")
        );
    }

    #[test]
    fn classification_hashes_single_file_url_image_by_extension() {
        let event = event(vec![
            data("public.file-url", b"file:///Users/example/tmp/abc.png"),
            data("public.tiff", &[0, 1, 2, 3]),
        ]);

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "png");
        assert_eq!(display_string(&classified), "PNG");
        assert_eq!(
            classified.content_hash,
            Database::hash_bytes(b"file:///Users/example/tmp/abc.png")
        );
    }

    #[test]
    fn classification_hashes_single_file_url_image_without_tiff() {
        let event = event(vec![data(
            "public.file-url",
            b"file:///Users/example/tmp/photo.HEIC",
        )]);

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "heic");
        assert_eq!(display_string(&classified), "HEIC");
        assert_eq!(
            classified.content_hash,
            Database::hash_bytes(b"file:///Users/example/tmp/photo.HEIC")
        );
    }

    #[test]
    fn classification_marks_single_file_url_video_even_with_empty_tiff() {
        let event = event(vec![
            data(
                "public.file-url",
                b"file:///Users/example/Desktop/Screen%20Recording.mov",
            ),
            data("public.tiff", &[]),
        ]);

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "video");
        assert_eq!(display_string(&classified), "Screen Recording.mov");
        assert_eq!(
            classified.content_hash,
            Database::hash_bytes(b"file:///Users/example/Desktop/Screen%20Recording.mov")
        );
    }

    #[test]
    fn classification_hashes_mixed_file_urls_in_order() {
        let event = Event {
            items: vec![
                Item {
                    data_list: vec![
                        data("public.utf8-plain-text", b"a.txt\rb"),
                        data("public.url", b"file:///tmp/a.txt"),
                        data("public.file-url", b"file:///tmp/a.txt"),
                    ],
                },
                Item {
                    data_list: vec![
                        data("public.file-url", b"file:///tmp/b/"),
                        data("public.url", b"file:///tmp/b/"),
                    ],
                },
            ],
        };

        let classified = Database::classify_event(&event).expect("event should classify");
        let mut hasher = Sha256::new();
        hasher.update(b"file:///tmp/a.txt");
        hasher.update(b"file:///tmp/b/");

        assert_eq!(classified.data_type, "files and folders");
        assert_eq!(
            display_file_items(&classified),
            vec![
                FileDisplayItem {
                    item_type: "file".to_string(),
                    name: "a.txt".to_string(),
                },
                FileDisplayItem {
                    item_type: "folder".to_string(),
                    name: "b".to_string(),
                },
            ]
        );
        assert_eq!(classified.content_hash, format!("{:x}", hasher.finalize()));
    }

    #[test]
    fn classification_uses_carriage_return_file_names_from_utf8_display() {
        let display = "claude-code-sourcemap-main.zip\rsqls\rssl\rtemp\r王德培-应聘登记表.doc";
        let event = Event {
            items: vec![
                Item {
                    data_list: vec![
                        data("public.utf8-plain-text", display.as_bytes()),
                        data("public.file-url", b"file:///.file/id=999999999.999999991"),
                    ],
                },
                Item {
                    data_list: vec![data(
                        "public.file-url",
                        b"file:///.file/id=999999999.999999992/",
                    )],
                },
                Item {
                    data_list: vec![data(
                        "public.file-url",
                        b"file:///.file/id=999999999.999999993/",
                    )],
                },
                Item {
                    data_list: vec![data(
                        "public.file-url",
                        b"file:///.file/id=999999999.999999994/",
                    )],
                },
                Item {
                    data_list: vec![data(
                        "public.file-url",
                        b"file:///.file/id=999999999.999999995",
                    )],
                },
            ],
        };

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "files and folders");
        assert_eq!(
            display_file_items(&classified),
            vec![
                FileDisplayItem {
                    item_type: "file".to_string(),
                    name: "claude-code-sourcemap-main.zip".to_string(),
                },
                FileDisplayItem {
                    item_type: "folder".to_string(),
                    name: "sqls".to_string(),
                },
                FileDisplayItem {
                    item_type: "folder".to_string(),
                    name: "ssl".to_string(),
                },
                FileDisplayItem {
                    item_type: "folder".to_string(),
                    name: "temp".to_string(),
                },
                FileDisplayItem {
                    item_type: "file".to_string(),
                    name: "王德培-应聘登记表.doc".to_string(),
                },
            ]
        );
    }

    #[test]
    fn classification_marks_multiple_file_urls_as_files() {
        let event = Event {
            items: vec![
                Item {
                    data_list: vec![
                        data("public.utf8-plain-text", b"2 files"),
                        data("public.file-url", b"file:///.file/id=999999999.999999999"),
                    ],
                },
                Item {
                    data_list: vec![data("public.file-url", b"file:///tmp/b.txt")],
                },
            ],
        };

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "files");
        assert_eq!(
            display_file_items(&classified),
            vec![
                FileDisplayItem {
                    item_type: "file".to_string(),
                    name: "File 1".to_string(),
                },
                FileDisplayItem {
                    item_type: "file".to_string(),
                    name: "b.txt".to_string(),
                },
            ]
        );
    }

    #[test]
    fn classification_marks_multiple_file_urls_as_folders() {
        let event = Event {
            items: vec![
                Item {
                    data_list: vec![
                        data("public.utf8-plain-text", b"2 folders"),
                        data("public.file-url", b"file:///.file/id=999999999.999999998/"),
                    ],
                },
                Item {
                    data_list: vec![data("public.file-url", b"file:///tmp/b/")],
                },
            ],
        };

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "folders");
        assert_eq!(
            display_file_items(&classified),
            vec![
                FileDisplayItem {
                    item_type: "folder".to_string(),
                    name: "Folder 1".to_string(),
                },
                FileDisplayItem {
                    item_type: "folder".to_string(),
                    name: "b".to_string(),
                },
            ]
        );
    }

    #[test]
    fn classification_hashes_single_utf8_plain_text_data() {
        let event = event(vec![data("public.utf8-plain-text", b"hello\nworld")]);

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "text");
        assert_eq!(display_string(&classified), "hello\nworld");
        assert_eq!(
            classified.content_hash,
            Database::hash_bytes(b"hello\nworld")
        );
    }

    #[test]
    fn classification_hashes_utf8_plain_text_when_private_metadata_is_present() {
        let event = event(vec![
            data("dyn.agk8", b"metadata"),
            data("org.chromium.source-url", b"https://example.test"),
            data("com.apple.webarchive", b"archive"),
            data("public.utf8-plain-text", b"hello\nworld"),
        ]);

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "text");
        assert_eq!(display_string(&classified), "hello\nworld");
        assert_eq!(
            classified.content_hash,
            Database::hash_bytes(b"hello\nworld")
        );
    }

    #[test]
    fn classification_falls_back_to_unsupported_for_unknown_data_types() {
        let event = event(vec![
            data("com.example.private-a", b"alpha"),
            data("com.example.private-b", b"beta"),
            data("com.example.private-c", b"gamma"),
            data("com.example.private-d", b"delta"),
        ]);
        let event_blob = encode_event_blob(&event).expect("event should encode");

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "unsupported");
        assert_eq!(
            display_string(&classified),
            "Unsupported clipboard data: com.example.private-a, com.example.private-b, com.example.private-c + 1 more"
        );
        assert_eq!(classified.content_hash, Database::hash_bytes(&event_blob));
    }

    #[test]
    fn event_blob_preserves_private_metadata_for_classified_events() {
        let event = event(vec![
            data("dyn.agk8", b"metadata"),
            data("org.chromium.source-url", b"https://example.test"),
            data("com.apple.webarchive", b"archive"),
            data("public.utf8-plain-text", b"hello\nworld"),
        ]);

        let blob = encode_event_blob(&event).expect("event should encode");
        let decoded = Database::event_from_blob(&blob).expect("event should decode");

        assert_eq!(decoded.items.len(), 1);
        assert_eq!(decoded.items[0].data_list.len(), 4);
        assert_eq!(decoded.items[0].data_list[0].r#type, "dyn.agk8");
        assert_eq!(
            decoded.items[0].data_list[1].r#type,
            "org.chromium.source-url"
        );
        assert_eq!(decoded.items[0].data_list[2].r#type, "com.apple.webarchive");
        assert_eq!(
            decoded.items[0].data_list[3].r#type,
            "public.utf8-plain-text"
        );
    }

    #[test]
    fn history_jsonl_writes_rows_with_truncated_data() {
        let db = in_memory_database();
        let path = temp_jsonl_path();
        let event = event(vec![
            data("dyn.binary", &[0xff, 0x00, 0x01, 0x02, 0x03, 0x04]),
            data("public.utf8-plain-text", b"hello world"),
        ]);

        db.insert_event(&event, None).expect("event should insert");
        db.write_history_jsonl(&HistoryJsonlConfig {
            path: path.clone(),
            max_data_bytes: 4,
        })
        .expect("JSONL should write");

        let contents = std::fs::read_to_string(&path).expect("JSONL should be readable");
        let _ = std::fs::remove_file(&path);
        let lines = contents.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 1);

        let value: serde_json::Value =
            serde_json::from_str(lines[0]).expect("JSONL row should be valid JSON");
        assert_eq!(value["data_type"], "text");
        assert_eq!(value["display"]["byte_len"], 11);
        assert_eq!(value["display"]["truncated"], true);
        assert_eq!(value["display"]["encoding"], "utf8");
        assert_eq!(value["display"]["value"], "hell");
        assert_eq!(
            value["event_data"]["items"][0]["data_list"][0]["type"],
            "dyn.binary"
        );
        assert_eq!(
            value["event_data"]["items"][0]["data_list"][0]["data"]["byte_len"],
            6
        );
        assert_eq!(
            value["event_data"]["items"][0]["data_list"][0]["data"]["truncated"],
            true
        );
        assert_eq!(
            value["event_data"]["items"][0]["data_list"][0]["data"]["encoding"],
            "hex"
        );
        assert_eq!(
            value["event_data"]["items"][0]["data_list"][0]["data"]["value"],
            "ff000102"
        );
    }

    #[test]
    fn history_jsonl_writes_unsupported_events_with_raw_data() {
        let db = in_memory_database();
        let path = temp_jsonl_path();
        let event = event(vec![data("com.example.private", &[0xde, 0xad, 0xbe, 0xef])]);

        db.insert_event(&event, None)
            .expect("unsupported event should insert");
        db.write_history_jsonl(&HistoryJsonlConfig {
            path: path.clone(),
            max_data_bytes: 128,
        })
        .expect("JSONL should write");

        let contents = std::fs::read_to_string(&path).expect("JSONL should be readable");
        let _ = std::fs::remove_file(&path);
        let lines = contents.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 1);

        let value: serde_json::Value =
            serde_json::from_str(lines[0]).expect("JSONL row should be valid JSON");
        assert_eq!(value["data_type"], "unsupported");
        assert_eq!(
            value["display"]["value"],
            "Unsupported clipboard data: com.example.private"
        );
        assert_eq!(
            value["event_data"]["items"][0]["data_list"][0]["type"],
            "com.example.private"
        );
        assert_eq!(
            value["event_data"]["items"][0]["data_list"][0]["data"]["encoding"],
            "hex"
        );
        assert_eq!(
            value["event_data"]["items"][0]["data_list"][0]["data"]["value"],
            "deadbeef"
        );
    }

    #[test]
    fn rich_preview_preserves_text_image_text_order() {
        let image_path = temp_png_path();
        let image_bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3];
        std::fs::write(&image_path, &image_bytes).expect("preview image should write");
        let file_url = format!("file://{}", image_path.display());
        let event = event(vec![
            data("public.utf8-plain-text", "文字1\n￼\n图片2".as_bytes()),
            data("public.file-url", file_url.as_bytes()),
        ]);

        let preview = Database::rich_preview_segments(&event);
        let _ = std::fs::remove_file(&image_path);

        assert_eq!(
            preview,
            vec![
                StoredPreviewSegment::Text {
                    text: "文字1".to_string(),
                },
                StoredPreviewSegment::Image {
                    label: image_path
                        .file_name()
                        .expect("image should have a file name")
                        .to_string_lossy()
                        .into_owned(),
                    media_type: "image/png".to_string(),
                    data: image_bytes,
                },
                StoredPreviewSegment::Text {
                    text: "图片2".to_string(),
                },
            ]
        );
    }

    #[test]
    fn rich_preview_requires_inline_attachment_placeholder() {
        let image_path = temp_png_path();
        let image_bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3];
        std::fs::write(&image_path, &image_bytes).expect("preview image should write");
        let file_url = format!("file://{}", image_path.display());
        let event = event(vec![
            data("public.utf8-plain-text", "plain image label".as_bytes()),
            data("public.file-url", file_url.as_bytes()),
        ]);

        let preview = Database::rich_preview_segments(&event);
        let _ = std::fs::remove_file(&image_path);

        assert!(preview.is_empty());
    }

    #[test]
    fn get_all_events_includes_rich_preview_segments() {
        let db = in_memory_database();
        let image_path = temp_png_path();
        let image_bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 4, 5, 6];
        std::fs::write(&image_path, &image_bytes).expect("preview image should write");
        let file_url = format!("file://{}", image_path.display());
        let event = event(vec![
            data("public.utf8-plain-text", "文字\n￼".as_bytes()),
            data("public.file-url", file_url.as_bytes()),
        ]);

        db.insert_event(&event, None).expect("event should insert");
        let events = db.get_all_events().expect("events should load");
        let _ = std::fs::remove_file(&image_path);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].rich_preview.len(), 2);
        assert_eq!(
            events[0].rich_preview[0],
            StoredPreviewSegment::Text {
                text: "文字".to_string(),
            }
        );
        assert_eq!(
            events[0].rich_preview[1],
            StoredPreviewSegment::Image {
                label: image_path
                    .file_name()
                    .expect("image should have a file name")
                    .to_string_lossy()
                    .into_owned(),
                media_type: "image/png".to_string(),
                data: image_bytes,
            }
        );
    }

    #[test]
    fn get_all_events_includes_video_preview_segment() {
        let db = in_memory_database();
        let video_path = temp_video_path();
        std::fs::write(&video_path, b"video placeholder").expect("preview video should write");
        let file_url = format!("file://{}", video_path.display());
        let event = event(vec![
            data("public.file-url", file_url.as_bytes()),
            data("public.tiff", &[]),
        ]);

        db.insert_event(&event, None).expect("event should insert");
        let events = db.get_all_events().expect("events should load");
        let _ = std::fs::remove_file(&video_path);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data_type, "video");
        assert_eq!(
            events[0].rich_preview,
            vec![StoredPreviewSegment::Video {
                label: video_path
                    .file_name()
                    .expect("video should have a file name")
                    .to_string_lossy()
                    .into_owned(),
                media_type: "video/quicktime".to_string(),
                path: video_path.to_string_lossy().into_owned(),
            }]
        );
    }
}
