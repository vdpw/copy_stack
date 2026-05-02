use crate::event::{decode_event_blob, encode_event_blob, event_from_legacy_json, ClipboardEvent};
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
    pub data_type: String,
    pub display: Vec<u8>,
    pub timestamp: i64,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AppSettings {
    pub max_items: u32,
    pub show_in_menu_bar: bool,
    pub move_restored_item_to_top: bool,
}

impl StoredEvent {
    fn new(content_hash: String, data_type: String, display: Vec<u8>, timestamp: i64) -> Self {
        Self {
            content_hash,
            data_type,
            display,
            timestamp,
        }
    }
}

struct DbRow {
    event_data: Vec<u8>,
    timestamp: i64,
}

struct ClassifiedEvent {
    content_hash: String,
    data_type: String,
    display: Vec<u8>,
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
            || !columns.iter().any(|column| column == "data_type")
            || !columns.iter().any(|column| column == "display")
            || !columns.iter().any(|column| column == "timestamp");
        let event_data_is_blob = self
            .column_declared_type("clipboard_events", "event_data")?
            .is_some_and(|column_type| column_type.eq_ignore_ascii_case("BLOB"));

        if has_legacy_columns
            || missing_required_columns
            || !event_data_is_blob
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
        let order_clause = if columns.iter().any(|column| column == "sort_order") {
            "ORDER BY sort_order DESC, timestamp DESC"
        } else {
            "ORDER BY timestamp DESC"
        };
        let query = format!(
            "SELECT event_data, timestamp FROM clipboard_events {}",
            order_clause
        );

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| {
            Ok(DbRow {
                event_data: Self::event_blob_from_row(row, 0)?,
                timestamp: Self::timestamp_from_row(row, 1)?,
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
            "SELECT event_data, timestamp
             FROM clipboard_events
             ORDER BY timestamp DESC, content_hash ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DbRow {
                event_data: Self::event_blob_from_row(row, 0)?,
                timestamp: row.get(1)?,
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
                    "INSERT INTO {} (content_hash, event_data, data_type, display, timestamp)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    table
                ),
                (
                    classified.content_hash,
                    row.event_data,
                    classified.data_type,
                    classified.display,
                    row.timestamp,
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

    pub fn insert_event(&self, event: &Event) -> Result<()> {
        let event_data = encode_event_blob(event)
            .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))?;
        let classified = Self::classify_event(event)?;

        let updated = self.conn.execute(
            "UPDATE clipboard_events
             SET event_data = ?1, data_type = ?2, display = ?3
             WHERE content_hash = ?4",
            (
                &event_data,
                &classified.data_type,
                &classified.display,
                &classified.content_hash,
            ),
        )?;

        if updated > 0 {
            return Ok(());
        }

        let timestamp = self.next_history_timestamp()?;
        self.conn.execute(
            "INSERT INTO clipboard_events (content_hash, event_data, data_type, display, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                classified.content_hash,
                event_data,
                classified.data_type,
                classified.display,
                timestamp,
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
        Self::classify_special_event(event).ok_or_else(|| {
            rusqlite::Error::InvalidParameterName(
                "unsupported clipboard event data types".to_string(),
            )
        })
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

        if let Some(data) = Self::find_data(event, "public.html") {
            return Some(Self::classified_from_single_data(
                "html",
                &data.data,
                Self::display_bytes(
                    Self::find_utf8_display(event).unwrap_or_else(|| "HTML".to_string()),
                ),
            ));
        }

        if let Some(data) = Self::find_data(event, "public.png") {
            return Some(Self::classified_from_single_data(
                "png",
                &data.data,
                b"PNG".to_vec(),
            ));
        }

        if event.items.len() > 1 {
            if let Some(file_urls) = Self::extract_multi_file_urls(event) {
                let data_type = Self::multi_file_url_data_type(&file_urls);
                let mut hasher = Sha256::new();
                for file_url in &file_urls {
                    hasher.update(file_url);
                }

                return Some(ClassifiedEvent {
                    content_hash: format!("{:x}", hasher.finalize()),
                    data_type: data_type.to_string(),
                    display: Self::find_utf8_display_in_item(&event.items[0])
                        .unwrap_or_else(|| Self::label_for_data_type(data_type))
                        .into_bytes(),
                });
            }
        }

        if event.items.len() == 1 {
            if let Some(data) = Self::find_data_in_item(&event.items[0], "public.file-url") {
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

                return Some(Self::classified_from_single_data(
                    data_type,
                    &data.data,
                    Self::display_bytes(
                        Self::find_utf8_display(event).unwrap_or_else(|| file_url.into_owned()),
                    ),
                ));
            }
        }

        Self::classify_plain_utf8_text(event)
    }

    fn classify_plain_utf8_text(event: &Event) -> Option<ClassifiedEvent> {
        if event.items.len() != 1 || event.items[0].data_list.len() != 1 {
            return None;
        }

        let data = &event.items[0].data_list[0];
        if data.r#type != "public.utf8-plain-text" {
            return None;
        }

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

        if Self::find_data_in_item(item, "public.tiff").is_some() {
            return Some("tiff".to_string());
        }

        None
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

    fn extract_multi_file_urls(event: &Event) -> Option<Vec<&[u8]>> {
        let mut file_urls = Vec::with_capacity(event.items.len());

        for (index, item) in event.items.iter().enumerate() {
            let file_url = Self::find_data_in_item(item, "public.file-url")?;
            let all_data_types_supported = item.data_list.iter().all(|data| {
                data.r#type == "public.file-url"
                    || (index == 0 && data.r#type == "public.utf8-plain-text")
            });

            if !all_data_types_supported {
                return None;
            }

            if index > 0 && item.data_list.len() != 1 {
                return None;
            }

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

    pub fn get_all_events(&self) -> Result<Vec<StoredEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT content_hash, data_type, display, timestamp
             FROM clipboard_events
             ORDER BY timestamp DESC, content_hash ASC",
        )?;

        let event_iter = stmt.query_map([], |row| {
            Ok(StoredEvent::new(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
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

    fn display_string(classified: &ClassifiedEvent) -> String {
        String::from_utf8_lossy(&classified.display).into_owned()
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
    fn classification_prefers_html_over_png() {
        let event = event(vec![
            data("public.utf8-plain-text", b"Visible text"),
            data("public.html", b"<p>Visible text</p>"),
            data("public.png", &[0, 1, 2]),
        ]);

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "html");
        assert_eq!(display_string(&classified), "Visible text");
        assert_eq!(
            classified.content_hash,
            Database::hash_bytes(b"<p>Visible text</p>")
        );
    }

    #[test]
    fn classification_marks_single_file_url_folder() {
        let event = event(vec![
            data("public.utf8-plain-text", b"/Users/example/Documents"),
            data("public.file-url", b"file:///Users/example/Documents/"),
        ]);

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "folder");
        assert_eq!(display_string(&classified), "/Users/example/Documents");
        assert_eq!(
            classified.content_hash,
            Database::hash_bytes(b"file:///Users/example/Documents/")
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
    fn classification_hashes_mixed_file_urls_in_order() {
        let event = Event {
            items: vec![
                Item {
                    data_list: vec![
                        data("public.utf8-plain-text", b"2 items"),
                        data("public.file-url", b"file:///tmp/a.txt"),
                    ],
                },
                Item {
                    data_list: vec![data("public.file-url", b"file:///tmp/b/")],
                },
            ],
        };

        let classified = Database::classify_event(&event).expect("event should classify");
        let mut hasher = Sha256::new();
        hasher.update(b"file:///tmp/a.txt");
        hasher.update(b"file:///tmp/b/");

        assert_eq!(classified.data_type, "files and folders");
        assert_eq!(display_string(&classified), "2 items");
        assert_eq!(classified.content_hash, format!("{:x}", hasher.finalize()));
    }

    #[test]
    fn classification_marks_multiple_file_urls_as_files() {
        let event = Event {
            items: vec![
                Item {
                    data_list: vec![
                        data("public.utf8-plain-text", b"2 files"),
                        data("public.file-url", b"file:///.file/id=6571367.66560150"),
                    ],
                },
                Item {
                    data_list: vec![data("public.file-url", b"file:///tmp/b.txt")],
                },
            ],
        };

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "files");
        assert_eq!(display_string(&classified), "2 files");
    }

    #[test]
    fn classification_marks_multiple_file_urls_as_folders() {
        let event = Event {
            items: vec![
                Item {
                    data_list: vec![
                        data("public.utf8-plain-text", b"2 folders"),
                        data("public.file-url", b"file:///.file/id=6571367.673004/"),
                    ],
                },
                Item {
                    data_list: vec![data("public.file-url", b"file:///tmp/b/")],
                },
            ],
        };

        let classified = Database::classify_event(&event).expect("event should classify");

        assert_eq!(classified.data_type, "folders");
        assert_eq!(display_string(&classified), "2 folders");
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
}
