use crate::event::{
    decode_event_blob, encode_event_blob, event_from_legacy_json, MAX_EVENT_BLOB_BYTES,
};
use crate::i18n::LanguagePreference;
use crate::pasteboard_protocol::{
    assess_event, PasteboardMetadata, REMOTE_CLIPBOARD_TYPE, SOURCE_TYPE,
};
use crate::resource_policy::MAX_DISPLAY_BYTES;
#[cfg(test)]
use crate::resource_policy::{
    MAX_DETAIL_IPC_BYTES, MAX_HTML_BYTES, MAX_PREVIEW_IMAGE_BYTES, MAX_PREVIEW_SEGMENTS,
};
use crate::store::classification::{
    self, ClassifiedEvent, FileDisplay, FileDisplayItem, FILE_DISPLAY_FORMAT,
};
use crate::store::models::{
    AppSettings, HistoryCursor, HistoryDetail, HistoryDetailSeed, HistoryPage, HistoryStats,
    HistorySummary, TrayEvent, DEFAULT_HISTORY_PAGE_SIZE, MAX_HISTORY_PAGE_SIZE,
    MAX_SUMMARY_DISPLAY_BYTES, TRAY_HISTORY_LIMIT,
};
use crate::store::preview;
#[cfg(test)]
use crate::store::preview::StoredPreviewSegment;
use crate::store::schema::{
    self, CLASSIFIER_METADATA_KEY, CLASSIFIER_METADATA_VERSION, CURRENT_SCHEMA_VERSION,
    REQUIRED_EVENT_COLUMNS,
};
use crate::store::settings;
use chrono::Utc;
use copy_event_listener::event::{Data, Event, Item};
use rusqlite::{
    params, types::ValueRef, Connection, OpenFlags, OptionalExtension, Result, Transaction,
};
#[cfg(test)]
use serde::Serialize;
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::collections::HashSet;
#[cfg(test)]
use std::fs::File;
#[cfg(test)]
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

const APP_DATA_DIR: &str = ".copy_stack";
const DB_FILE_NAME: &str = "copy_stack.db";
const MAX_SOURCE_BUNDLE_ID_BYTES: usize = 255;
#[cfg(test)]
const INLINE_ATTACHMENT_PLACEHOLDER: char = '\u{fffc}';

#[cfg(test)]
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredEvent {
    pub content_hash: String,
    pub data_type: String,
    pub display: Vec<u8>,
    pub html_preview: Option<String>,
    pub rich_preview: Vec<StoredPreviewSegment>,
    pub timestamp: i64,
}

#[cfg(test)]
impl StoredEvent {
    fn new(
        content_hash: String,
        data_type: String,
        display: Vec<u8>,
        html_preview: Option<String>,
        rich_preview: Vec<StoredPreviewSegment>,
        timestamp: i64,
    ) -> Self {
        Self {
            content_hash,
            data_type,
            display,
            html_preview,
            rich_preview,
            timestamp,
        }
    }
}

struct DbRow {
    event_data: Vec<u8>,
    timestamp: i64,
}

struct PersistedMetadata {
    source_bundle_id: Option<String>,
    is_remote_clipboard: bool,
    summary_display: Vec<u8>,
    summary_truncated: bool,
    compact_content_hash: Option<String>,
    compact_display: Option<Vec<u8>>,
    byte_count: u64,
}

pub(crate) struct PreparedHistoryEvent {
    event: Event,
    event_data: Vec<u8>,
    classified: ClassifiedEvent,
    metadata: PersistedMetadata,
    compact_mode: bool,
}

impl PreparedHistoryEvent {
    pub(crate) fn content_hash(&self) -> &str {
        &self.classified.content_hash
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreSeed {
    pub content_hash: String,
    pub event_data: Vec<u8>,
    pub source_bundle_id: Option<String>,
    pub is_remote_clipboard: bool,
    compact_mode: bool,
}

impl RestoreSeed {
    /// Decodes and applies the current compact projection after the database
    /// lock has been released.
    pub fn into_event(self) -> Result<Option<Event>> {
        let event = Database::event_from_blob(&self.event_data)?;
        if self.compact_mode {
            Ok(Database::compact_text_event(&event))
        } else {
            Ok(Some(event))
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MigrationStats {
    source_rows: u64,
    inserted_rows: u64,
    duplicate_rows: u64,
    policy_dropped_rows: u64,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MigrationFailpoint {
    AfterCreateReplacement,
    AfterCopy,
    AfterValidation,
    AfterDropOriginal,
}

#[derive(Clone, Debug)]
pub struct HistoryJsonlConfig {
    pub path: PathBuf,
    pub max_data_bytes: usize,
}

#[cfg(test)]
#[derive(Serialize)]
struct HistoryJsonlRecord {
    content_hash: String,
    data_type: String,
    timestamp: i64,
    display: HistoryJsonlBytes,
    event_data: HistoryJsonlEvent,
}

#[cfg(test)]
#[derive(Serialize)]
struct HistoryJsonlEvent {
    items: Vec<HistoryJsonlItem>,
}

#[cfg(test)]
#[derive(Serialize)]
struct HistoryJsonlItem {
    data_list: Vec<HistoryJsonlData>,
}

#[cfg(test)]
#[derive(Serialize)]
struct HistoryJsonlData {
    #[serde(rename = "type")]
    data_type: String,
    data: HistoryJsonlBytes,
}

#[cfg(test)]
#[derive(Serialize)]
struct HistoryJsonlBytes {
    byte_len: usize,
    truncated: bool,
    encoding: &'static str,
    value: String,
}

pub struct Database {
    conn: Connection,
    path: Option<PathBuf>,
}

#[cfg(test)]
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

#[cfg(test)]
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
        Self::open_private_database(&db_path)
    }

    #[cfg(test)]
    pub(crate) fn open_path(path: &std::path::Path) -> Result<Self> {
        Self::open_private_database(path)
    }

    fn database_path() -> Result<PathBuf> {
        #[cfg(debug_assertions)]
        if let Some(qa_data_dir) = std::env::var_os("COPY_STACK_QA_DATA_DIR") {
            return Self::qa_database_path(Path::new(&qa_data_dir));
        }

        let home_dir = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
            rusqlite::Error::InvalidParameterName(
                "database home directory is unavailable".to_string(),
            )
        })?;
        Ok(home_dir.join(APP_DATA_DIR).join(DB_FILE_NAME))
    }

    #[cfg(any(debug_assertions, test))]
    fn qa_database_path(data_dir: &Path) -> Result<PathBuf> {
        if !data_dir.is_absolute() {
            return Err(rusqlite::Error::InvalidParameterName(
                "QA data directory must be absolute".to_string(),
            ));
        }
        Ok(data_dir.join(DB_FILE_NAME))
    }

    fn open_private_database(path: &Path) -> Result<Self> {
        let path = crate::private_fs::prepare_sqlite_database(path)
            .map_err(|_| Self::private_database_error("prepare"))?;
        let db = Self {
            conn: Connection::open(&path)?,
            path: Some(path.clone()),
        };

        let schema_result = db.initialize_schema();
        let hardening_result = crate::private_fs::harden_sqlite_files(&path)
            .map_err(|_| Self::private_database_error("harden"));
        schema_result?;
        hardening_result?;
        Ok(db)
    }

    fn private_database_error(operation: &'static str) -> rusqlite::Error {
        rusqlite::Error::InvalidParameterName(format!(
            "private database {operation} operation failed"
        ))
    }

    fn initialize_schema(&self) -> Result<()> {
        self.initialize_schema_with_failpoint(None)
    }

    fn initialize_schema_with_failpoint(
        &self,
        failpoint: Option<MigrationFailpoint>,
    ) -> Result<()> {
        let transaction = self.conn.unchecked_transaction()?;
        let schema_version = schema::user_version(&transaction)?;
        if schema_version > CURRENT_SCHEMA_VERSION {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "database schema version {schema_version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
            )));
        }

        schema::create_settings_table(&transaction)?;
        schema::create_metadata_table(&transaction)?;
        Self::insert_default_settings(&transaction)?;

        let table_exists = Self::table_exists_in(&transaction, "clipboard_events")?;
        let columns = if table_exists {
            Self::table_columns_in(&transaction, "clipboard_events")?
        } else {
            Vec::new()
        };
        let classifier_version = Self::metadata_version_in(&transaction, CLASSIFIER_METADATA_KEY)?;
        if classifier_version > CLASSIFIER_METADATA_VERSION {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "classifier metadata version {classifier_version} is newer than supported version {CLASSIFIER_METADATA_VERSION}"
            )));
        }
        let current_shape =
            table_exists && Self::clipboard_events_schema_is_current(&transaction, &columns)?;

        let rebuilt_history = if !table_exists {
            schema::create_clipboard_events_table(&transaction, "clipboard_events")?;
            true
        } else if schema_version < CURRENT_SCHEMA_VERSION
            || !current_shape
            || classifier_version < CLASSIFIER_METADATA_VERSION
        {
            Self::rebuild_clipboard_events_table_in(&transaction, &columns, failpoint)?;
            true
        } else {
            false
        };

        if rebuilt_history {
            schema::drop_clipboard_event_indexes(&transaction)?;
        }
        schema::create_clipboard_event_indexes(&transaction)?;
        Self::validate_clipboard_event_indexes(&transaction)?;
        Self::validate_clipboard_events_table(&transaction, "clipboard_events", rebuilt_history)?;
        Self::set_metadata_version_in(
            &transaction,
            CLASSIFIER_METADATA_KEY,
            CLASSIFIER_METADATA_VERSION,
        )?;
        schema::set_user_version(&transaction, CURRENT_SCHEMA_VERSION)?;
        transaction.commit()
    }

    fn insert_default_settings(connection: &Connection) -> Result<()> {
        for (key, value) in settings::default_entries() {
            connection.execute(
                "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
                params![key, value],
            )?;
        }
        Ok(())
    }

    fn metadata_version_in(connection: &Connection, key: &str) -> Result<i64> {
        connection
            .query_row(
                "SELECT value FROM app_metadata WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()
            .map(|value| value.unwrap_or(0))
    }

    fn set_metadata_version_in(connection: &Connection, key: &str, version: i64) -> Result<()> {
        connection.execute(
            "INSERT INTO app_metadata (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, version],
        )?;
        Ok(())
    }

    fn table_exists_in(connection: &Connection, table: &str) -> Result<bool> {
        let mut stmt =
            connection.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
        let mut rows = stmt.query([table])?;
        Ok(rows.next()?.is_some())
    }

    #[cfg(test)]
    fn table_columns(&self, table: &str) -> Result<Vec<String>> {
        Self::table_columns_in(&self.conn, table)
    }

    fn table_columns_in(connection: &Connection, table: &str) -> Result<Vec<String>> {
        let pragma = format!("PRAGMA table_info({})", table);
        let mut stmt = connection.prepare(&pragma)?;
        let mut rows = stmt.query([])?;
        let mut columns = Vec::new();

        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            columns.push(name);
        }

        Ok(columns)
    }

    fn column_declared_type_in(
        connection: &Connection,
        table: &str,
        expected_column: &str,
    ) -> Result<Option<String>> {
        let pragma = format!("PRAGMA table_info({})", table);
        let mut stmt = connection.prepare(&pragma)?;
        let mut rows = stmt.query([])?;

        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == expected_column {
                return Ok(Some(row.get(2)?));
            }
        }

        Ok(None)
    }

    fn primary_key_column_is_in(
        connection: &Connection,
        table: &str,
        expected_column: &str,
    ) -> Result<bool> {
        let pragma = format!("PRAGMA table_info({})", table);
        let mut stmt = connection.prepare(&pragma)?;
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

    #[cfg(test)]
    fn rebuild_clipboard_events_table(&self, columns: &[String]) -> Result<()> {
        let transaction = self.conn.unchecked_transaction()?;
        Self::rebuild_clipboard_events_table_in(&transaction, columns, None)?;
        schema::drop_clipboard_event_indexes(&transaction)?;
        schema::create_clipboard_event_indexes(&transaction)?;
        Self::validate_clipboard_event_indexes(&transaction)?;
        Self::validate_clipboard_events_table(&transaction, "clipboard_events", true)?;
        Self::set_metadata_version_in(
            &transaction,
            CLASSIFIER_METADATA_KEY,
            CLASSIFIER_METADATA_VERSION,
        )?;
        schema::set_user_version(&transaction, CURRENT_SCHEMA_VERSION)?;
        transaction.commit()
    }

    fn read_clipboard_event_rows_in(
        connection: &Connection,
        columns: &[String],
    ) -> Result<Vec<DbRow>> {
        let has_sort_order = columns.iter().any(|column| column == "sort_order");
        let order_clause = if has_sort_order {
            "ORDER BY sort_order DESC, timestamp DESC"
        } else {
            "ORDER BY timestamp DESC, rowid DESC"
        };
        let query = format!(
            "SELECT event_data, timestamp FROM clipboard_events {}",
            order_clause
        );

        let mut stmt = connection.prepare(&query)?;
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

        // Older schemas used `sort_order` as the authoritative history order,
        // including when a restored item deliberately kept its position while
        // retaining a newer wall-clock timestamp. The current schema uses the
        // timestamp plus hash cursor as its ordering key, so translate the
        // already-sorted legacy rows into adjacent millisecond ranks. This
        // preserves the exact legacy order while shifting timestamps by at
        // most the migrated row count.
        if has_sort_order && event_rows.len() > 1 {
            let newest_timestamp = event_rows
                .iter()
                .map(|row| row.timestamp)
                .max()
                .unwrap_or_default();
            let top_timestamp = newest_timestamp.saturating_add(event_rows.len() as i64);
            for (index, row) in event_rows.iter_mut().enumerate() {
                row.timestamp = top_timestamp.saturating_sub(index as i64);
            }
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
        if timestamp.unsigned_abs() < 10_000_000_000 {
            timestamp.saturating_mul(1000)
        } else {
            timestamp
        }
    }

    #[cfg(test)]
    fn rebuild_history_metadata(&self) -> Result<()> {
        let columns = self.table_columns("clipboard_events")?;
        self.rebuild_clipboard_events_table(&columns)
    }

    fn clipboard_events_schema_is_current(
        connection: &Connection,
        columns: &[String],
    ) -> Result<bool> {
        Ok(columns.len() == REQUIRED_EVENT_COLUMNS.len()
            && REQUIRED_EVENT_COLUMNS
                .iter()
                .all(|required| columns.iter().any(|column| column == required))
            && Self::column_declared_type_in(connection, "clipboard_events", "event_data")?
                .is_some_and(|column_type| column_type.eq_ignore_ascii_case("BLOB"))
            && Self::column_declared_type_in(connection, "clipboard_events", "display")?
                .is_some_and(|column_type| column_type.eq_ignore_ascii_case("BLOB"))
            && Self::primary_key_column_is_in(connection, "clipboard_events", "content_hash")?)
    }

    fn rebuild_clipboard_events_table_in(
        transaction: &Transaction<'_>,
        columns: &[String],
        failpoint: Option<MigrationFailpoint>,
    ) -> Result<MigrationStats> {
        transaction.execute("DROP TABLE IF EXISTS clipboard_events_next", [])?;
        schema::create_clipboard_events_table(transaction, "clipboard_events_next")?;
        Self::maybe_fail_migration(failpoint, MigrationFailpoint::AfterCreateReplacement)?;

        let rows = Self::read_clipboard_event_rows_in(transaction, columns)?;
        let stats = Self::insert_deduped_rows_in(transaction, "clipboard_events_next", rows)?;
        Self::maybe_fail_migration(failpoint, MigrationFailpoint::AfterCopy)?;

        Self::validate_clipboard_events_table(transaction, "clipboard_events_next", true)?;
        let replacement_count: u64 =
            transaction.query_row("SELECT COUNT(*) FROM clipboard_events_next", [], |row| {
                row.get(0)
            })?;
        if replacement_count != stats.inserted_rows
            || stats.source_rows
                != stats.inserted_rows + stats.duplicate_rows + stats.policy_dropped_rows
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "clipboard history migration row accounting failed".to_string(),
            ));
        }
        Self::maybe_fail_migration(failpoint, MigrationFailpoint::AfterValidation)?;

        schema::drop_clipboard_event_indexes(transaction)?;
        transaction.execute("DROP TABLE clipboard_events", [])?;
        Self::maybe_fail_migration(failpoint, MigrationFailpoint::AfterDropOriginal)?;
        transaction.execute(
            "ALTER TABLE clipboard_events_next RENAME TO clipboard_events",
            [],
        )?;
        Ok(stats)
    }

    fn insert_deduped_rows_in(
        connection: &Connection,
        table: &str,
        rows: Vec<DbRow>,
    ) -> Result<MigrationStats> {
        let mut stats = MigrationStats {
            source_rows: rows.len() as u64,
            ..MigrationStats::default()
        };
        let mut seen_hashes = HashSet::new();

        for row in rows {
            let event = Self::event_from_blob(&row.event_data)?;
            let assessment = assess_event(&event);
            if !assessment.should_record() {
                stats.policy_dropped_rows += 1;
                continue;
            }
            let Some(classified) = Self::classify_event(&event) else {
                stats.policy_dropped_rows += 1;
                continue;
            };
            if classified.content_hash.is_empty()
                || !seen_hashes.insert(classified.content_hash.clone())
            {
                stats.duplicate_rows += 1;
                continue;
            }
            let compact_classified = Self::compact_text_event(&event)
                .as_ref()
                .and_then(Self::classify_event);
            let metadata = Self::persisted_metadata(
                &row.event_data,
                &classified,
                assessment.metadata,
                compact_classified,
            );

            connection.execute(
                &format!(
                    "INSERT INTO {table} (
                        content_hash,
                        event_data,
                        data_type,
                        display,
                        summary_display,
                        summary_truncated,
                        compact_content_hash,
                        compact_display,
                        source_bundle_id,
                        is_remote_clipboard,
                        byte_count,
                        timestamp,
                        metadata_version
                     ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
                     )"
                ),
                params![
                    classified.content_hash,
                    row.event_data,
                    classified.data_type,
                    classified.display,
                    metadata.summary_display,
                    metadata.summary_truncated,
                    metadata.compact_content_hash,
                    metadata.compact_display,
                    metadata.source_bundle_id,
                    metadata.is_remote_clipboard,
                    metadata.byte_count,
                    row.timestamp,
                    CLASSIFIER_METADATA_VERSION,
                ],
            )?;
            stats.inserted_rows += 1;
        }

        Ok(stats)
    }

    fn validate_clipboard_events_table(
        connection: &Connection,
        table: &str,
        validate_rows: bool,
    ) -> Result<()> {
        let columns = Self::table_columns_in(connection, table)?;
        if columns.len() != REQUIRED_EVENT_COLUMNS.len()
            || !REQUIRED_EVENT_COLUMNS
                .iter()
                .all(|required| columns.iter().any(|column| column == required))
            || !Self::primary_key_column_is_in(connection, table, "content_hash")?
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "clipboard history schema validation failed".to_string(),
            ));
        }

        if !validate_rows {
            return Ok(());
        }

        let invalid_rows: u64 = connection.query_row(
            &format!(
                "SELECT COUNT(*) FROM {table}
                 WHERE length(content_hash) != 64
                    OR summary_display IS NULL
                    OR byte_count < 0
                    OR metadata_version != ?1"
            ),
            [CLASSIFIER_METADATA_VERSION],
            |row| row.get(0),
        )?;
        if invalid_rows != 0 {
            return Err(rusqlite::Error::InvalidParameterName(
                "clipboard history metadata validation failed".to_string(),
            ));
        }
        Ok(())
    }

    fn validate_clipboard_event_indexes(connection: &Connection) -> Result<()> {
        for index in [
            "idx_clipboard_events_timestamp",
            "idx_clipboard_events_compact",
        ] {
            let exists = connection
                .query_row(
                    "SELECT 1
                     FROM sqlite_master
                     WHERE type = 'index' AND name = ?1 AND tbl_name = 'clipboard_events'",
                    [index],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !exists {
                return Err(rusqlite::Error::InvalidParameterName(
                    "clipboard history index validation failed".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn maybe_fail_migration(
        actual: Option<MigrationFailpoint>,
        expected: MigrationFailpoint,
    ) -> Result<()> {
        if actual == Some(expected) {
            Err(rusqlite::Error::InvalidParameterName(
                "injected clipboard history migration failure".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    pub fn get_settings(&self) -> Result<AppSettings> {
        let language = self.get_language()?;
        let max_history_bytes = self.get_max_history_bytes()?;
        let history = self.get_history_stats()?;
        Ok(AppSettings {
            max_items: self.get_max_items()?,
            max_history_bytes,
            show_in_menu_bar: self.get_show_in_menu_bar()?,
            move_restored_item_to_top: self.get_move_restored_item_to_top()?,
            compact_mode: self.get_compact_mode()?,
            language: language.code().to_string(),
            resolved_language: language.resolve().code().to_string(),
            history_count: history.total_items,
            history_bytes: history.total_bytes,
            history_limit_bytes: max_history_bytes,
            max_event_bytes: MAX_EVENT_BLOB_BYTES as u64,
        })
    }

    pub fn get_max_items(&self) -> Result<u32> {
        settings::get_max_items(&self.conn)
    }

    pub fn set_max_items(&self, max_items: u32) -> Result<()> {
        settings::set_max_items(&self.conn, max_items)
    }

    pub fn get_max_history_bytes(&self) -> Result<u64> {
        settings::get_max_history_bytes(&self.conn)
    }

    pub fn set_max_history_bytes(&self, max_history_bytes: u64) -> Result<()> {
        settings::set_max_history_bytes(&self.conn, max_history_bytes)
    }

    pub fn get_show_in_menu_bar(&self) -> Result<bool> {
        settings::get_show_in_menu_bar(&self.conn)
    }

    pub fn set_show_in_menu_bar(&self, show_in_menu_bar: bool) -> Result<()> {
        settings::set_show_in_menu_bar(&self.conn, show_in_menu_bar)
    }

    pub fn get_move_restored_item_to_top(&self) -> Result<bool> {
        settings::get_move_restored_item_to_top(&self.conn)
    }

    pub fn set_move_restored_item_to_top(&self, move_restored_item_to_top: bool) -> Result<()> {
        settings::set_move_restored_item_to_top(&self.conn, move_restored_item_to_top)
    }

    pub fn get_compact_mode(&self) -> Result<bool> {
        settings::get_compact_mode(&self.conn)
    }

    pub fn set_compact_mode(&self, compact_mode: bool) -> Result<()> {
        settings::set_compact_mode(&self.conn, compact_mode)
    }

    pub(crate) fn get_language(&self) -> Result<LanguagePreference> {
        settings::get_language(&self.conn)
    }

    pub(crate) fn set_language(&self, language: LanguagePreference) -> Result<()> {
        settings::set_language(&self.conn, language)
    }

    pub(crate) fn prepare_history_event(
        event: &Event,
        compact_mode: bool,
    ) -> Result<Option<PreparedHistoryEvent>> {
        let assessment = assess_event(event);
        if !assessment.should_record() {
            return Ok(None);
        }

        let prepared_event = if compact_mode {
            let Some(compact_event) = Self::compact_text_event(event) else {
                return Ok(None);
            };
            compact_event
        } else {
            event.clone()
        };
        let Some(mut classified) = Self::classify_event(&prepared_event) else {
            return Ok(None);
        };
        classified.display =
            Self::bounded_persisted_display(&classified.data_type, &classified.display);
        let event_data = encode_event_blob(&prepared_event).map_err(|_| {
            rusqlite::Error::InvalidParameterName(
                "clipboard event could not be prepared for storage".to_string(),
            )
        })?;
        let mut compact_classified = if compact_mode {
            Some(classified.clone())
        } else {
            Self::compact_text_event(event)
                .as_ref()
                .and_then(Self::classify_event)
        };
        if let Some(compact) = compact_classified.as_mut() {
            compact.display = Self::bounded_persisted_display(&compact.data_type, &compact.display);
        }
        let metadata = Self::persisted_metadata(
            &event_data,
            &classified,
            assessment.metadata,
            compact_classified,
        );

        Ok(Some(PreparedHistoryEvent {
            event: prepared_event,
            event_data,
            classified,
            metadata,
            compact_mode,
        }))
    }

    #[cfg(test)]
    pub fn insert_event(&self, event: &Event) -> Result<bool> {
        let compact_mode = self.get_compact_mode()?;
        let Some(prepared) = Self::prepare_history_event(event, compact_mode)? else {
            return Ok(false);
        };
        self.insert_prepared_event(prepared)
    }

    pub(crate) fn insert_prepared_event(&self, prepared: PreparedHistoryEvent) -> Result<bool> {
        let defense = assess_event(&prepared.event);
        if !defense.should_record() {
            return Ok(false);
        }

        let expected_source_bundle_id = defense
            .metadata
            .source_bundle_id
            .filter(|source| source.len() <= MAX_SOURCE_BUNDLE_ID_BYTES);
        if expected_source_bundle_id != prepared.metadata.source_bundle_id
            || defense.metadata.is_remote_clipboard != prepared.metadata.is_remote_clipboard
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "prepared clipboard metadata validation failed".to_string(),
            ));
        }

        let PreparedHistoryEvent {
            event: _,
            event_data,
            classified,
            metadata,
            compact_mode,
        } = prepared;

        if compact_mode {
            return self.upsert_compact_event(event_data, classified, metadata);
        }

        let transaction = self.conn.unchecked_transaction()?;
        let updated = transaction.execute(
            "UPDATE clipboard_events
             SET event_data = ?1,
                 data_type = ?2,
                 display = ?3,
                 summary_display = ?4,
                 summary_truncated = ?5,
                 compact_content_hash = ?6,
                 compact_display = ?7,
                 source_bundle_id = ?8,
                 is_remote_clipboard = ?9,
                 byte_count = ?10,
                 metadata_version = ?11
             WHERE content_hash = ?12",
            params![
                &event_data,
                &classified.data_type,
                &classified.display,
                &metadata.summary_display,
                metadata.summary_truncated,
                &metadata.compact_content_hash,
                &metadata.compact_display,
                &metadata.source_bundle_id,
                metadata.is_remote_clipboard,
                metadata.byte_count,
                CLASSIFIER_METADATA_VERSION,
                &classified.content_hash,
            ],
        )?;

        if updated == 0 {
            let timestamp = Self::next_history_timestamp_in(&transaction)?;
            Self::insert_current_row(&transaction, &classified, &event_data, &metadata, timestamp)?;
        }

        Self::cleanup_old_events_in(&transaction)?;
        transaction.commit()?;
        Ok(true)
    }

    fn upsert_compact_event(
        &self,
        event_data: Vec<u8>,
        classified: ClassifiedEvent,
        metadata: PersistedMetadata,
    ) -> Result<bool> {
        let transaction = self.conn.unchecked_transaction()?;
        let mut stmt = transaction.prepare(
            "SELECT content_hash, timestamp
             FROM clipboard_events
             WHERE compact_content_hash = ?1
             ORDER BY timestamp DESC, content_hash ASC",
        )?;
        let rows = stmt.query_map([&classified.content_hash], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut matching_rows = Vec::new();
        for row in rows {
            matching_rows.push(row?);
        }
        drop(stmt);

        if let Some((newest_content_hash, newest_timestamp)) = matching_rows.first() {
            let existing_target_hash = matching_rows
                .iter()
                .find(|(content_hash, _)| content_hash == &classified.content_hash)
                .map(|(content_hash, _)| content_hash.as_str());
            let row_to_update = existing_target_hash.unwrap_or(newest_content_hash);
            for (content_hash, _) in &matching_rows {
                if content_hash != row_to_update {
                    transaction.execute(
                        "DELETE FROM clipboard_events WHERE content_hash = ?1",
                        [content_hash],
                    )?;
                }
            }
            transaction.execute(
                "UPDATE clipboard_events
                 SET content_hash = ?1,
                     event_data = ?2,
                     data_type = ?3,
                     display = ?4,
                     summary_display = ?5,
                     summary_truncated = ?6,
                     compact_content_hash = ?7,
                     compact_display = ?8,
                     source_bundle_id = ?9,
                     is_remote_clipboard = ?10,
                     byte_count = ?11,
                     timestamp = ?12,
                     metadata_version = ?13
                 WHERE content_hash = ?14",
                params![
                    &classified.content_hash,
                    &event_data,
                    &classified.data_type,
                    &classified.display,
                    &metadata.summary_display,
                    metadata.summary_truncated,
                    &metadata.compact_content_hash,
                    &metadata.compact_display,
                    &metadata.source_bundle_id,
                    metadata.is_remote_clipboard,
                    metadata.byte_count,
                    newest_timestamp,
                    CLASSIFIER_METADATA_VERSION,
                    row_to_update,
                ],
            )?;
            Self::cleanup_old_events_in(&transaction)?;
            transaction.commit()?;
            return Ok(true);
        }

        let timestamp = Self::next_history_timestamp_in(&transaction)?;
        Self::insert_current_row(&transaction, &classified, &event_data, &metadata, timestamp)?;
        Self::cleanup_old_events_in(&transaction)?;
        transaction.commit()?;
        Ok(true)
    }

    fn insert_current_row(
        connection: &Connection,
        classified: &ClassifiedEvent,
        event_data: &[u8],
        metadata: &PersistedMetadata,
        timestamp: i64,
    ) -> Result<()> {
        connection.execute(
            "INSERT INTO clipboard_events (
                content_hash,
                event_data,
                data_type,
                display,
                summary_display,
                summary_truncated,
                compact_content_hash,
                compact_display,
                source_bundle_id,
                is_remote_clipboard,
                byte_count,
                timestamp,
                metadata_version
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
             )",
            params![
                &classified.content_hash,
                event_data,
                &classified.data_type,
                &classified.display,
                &metadata.summary_display,
                metadata.summary_truncated,
                &metadata.compact_content_hash,
                &metadata.compact_display,
                &metadata.source_bundle_id,
                metadata.is_remote_clipboard,
                metadata.byte_count,
                timestamp,
                CLASSIFIER_METADATA_VERSION,
            ],
        )?;
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
        Self::next_history_timestamp_in(&self.conn)
    }

    fn next_history_timestamp_in(connection: &Connection) -> Result<i64> {
        let max_timestamp: i64 = connection.query_row(
            "SELECT COALESCE(MAX(timestamp), 0) FROM clipboard_events",
            [],
            |row| row.get(0),
        )?;
        let next_timestamp = max_timestamp.checked_add(1).ok_or_else(|| {
            rusqlite::Error::InvalidParameterName(
                "clipboard history timestamp range exhausted".to_string(),
            )
        })?;
        Ok(Self::current_unix_timestamp().max(next_timestamp))
    }

    fn current_unix_timestamp() -> i64 {
        Utc::now().timestamp_millis()
    }

    #[cfg(test)]
    pub fn event_content_hash(&self, event: &Event) -> Result<Option<String>> {
        if !assess_event(event).should_record() {
            return Ok(None);
        }
        if self.get_compact_mode()? {
            let Some(event) = Self::compact_text_event(event) else {
                return Ok(None);
            };
            return Ok(Self::classify_event(&event).map(|classified| classified.content_hash));
        }

        Ok(Self::classify_event(event).map(|classified| classified.content_hash))
    }

    fn classify_event(event: &Event) -> Option<ClassifiedEvent> {
        classification::classify_event(event)
    }

    pub fn parse_file_display(display: &[u8]) -> Option<FileDisplay> {
        classification::parse_file_display(display)
    }

    fn persisted_metadata(
        event_data: &[u8],
        classified: &ClassifiedEvent,
        protocol_metadata: PasteboardMetadata,
        compact_classified: Option<ClassifiedEvent>,
    ) -> PersistedMetadata {
        let source_bundle_id = protocol_metadata
            .source_bundle_id
            .filter(|source| source.len() <= MAX_SOURCE_BUNDLE_ID_BYTES);
        let (summary_display, summary_truncated) =
            Self::bounded_summary_display(&classified.data_type, &classified.display);
        let compact_content_hash = compact_classified
            .as_ref()
            .map(|compact| compact.content_hash.clone());
        let compact_display = compact_classified.map(|compact| compact.display);
        let byte_count = [
            event_data.len() as u64,
            classified.display.len() as u64,
            summary_display.len() as u64,
            compact_display
                .as_ref()
                .map_or(0, |display| display.len() as u64),
            source_bundle_id
                .as_ref()
                .map_or(0, |source| source.len() as u64),
        ]
        .into_iter()
        .fold(0_u64, u64::saturating_add);

        PersistedMetadata {
            source_bundle_id,
            is_remote_clipboard: protocol_metadata.is_remote_clipboard,
            summary_display,
            summary_truncated,
            compact_content_hash,
            compact_display,
            byte_count,
        }
    }

    fn bounded_summary_display(data_type: &str, display: &[u8]) -> (Vec<u8>, bool) {
        if let Some(file_display) = Self::parse_file_display(display) {
            return Self::bounded_file_summary(file_display, display.len());
        }

        if let Ok(text) = std::str::from_utf8(display) {
            return Self::bounded_utf8_summary(text, MAX_SUMMARY_DISPLAY_BYTES);
        }

        let fallback = match data_type {
            "png" => "PNG".to_string(),
            value if value.is_empty() => "CLIPBOARD ITEM".to_string(),
            value => value.to_ascii_uppercase(),
        };
        (fallback.into_bytes(), true)
    }

    fn bounded_persisted_display(data_type: &str, display: &[u8]) -> Vec<u8> {
        if display.len() <= MAX_DISPLAY_BYTES {
            return display.to_vec();
        }
        if let Ok(text) = std::str::from_utf8(display) {
            return Self::bounded_utf8_summary(text, MAX_DISPLAY_BYTES).0;
        }
        classification::label_for_data_type(data_type).into_bytes()
    }

    fn bounded_file_summary(file_display: FileDisplay, original_len: usize) -> (Vec<u8>, bool) {
        let item_count = file_display.items.len();
        let mut summary_items = Vec::new();
        let mut changed = original_len > MAX_SUMMARY_DISPLAY_BYTES;

        for item in file_display.items {
            let (name, name_truncated) =
                Self::bounded_utf8_summary(&item.name, MAX_SUMMARY_DISPLAY_BYTES / 3);
            changed |= name_truncated;
            let item = FileDisplayItem {
                item_type: item.item_type,
                name: String::from_utf8(name).unwrap_or_default(),
            };
            let mut candidate = summary_items.clone();
            candidate.push(item.clone());
            let encoded = serde_json::to_vec(&FileDisplay {
                format: FILE_DISPLAY_FORMAT.to_string(),
                items: candidate,
            })
            .unwrap_or_default();
            if encoded.len() > MAX_SUMMARY_DISPLAY_BYTES {
                changed = true;
                break;
            }
            summary_items.push(item);
        }

        changed |= summary_items.len() != item_count;
        let encoded = serde_json::to_vec(&FileDisplay {
            format: FILE_DISPLAY_FORMAT.to_string(),
            items: summary_items,
        })
        .unwrap_or_else(|_| b"Files".to_vec());
        if encoded.len() <= MAX_SUMMARY_DISPLAY_BYTES {
            (encoded, changed)
        } else {
            (b"Files".to_vec(), true)
        }
    }

    fn bounded_utf8_summary(value: &str, max_bytes: usize) -> (Vec<u8>, bool) {
        if value.len() <= max_bytes {
            return (value.as_bytes().to_vec(), false);
        }

        const SUFFIX: &str = "...";
        let available = max_bytes.saturating_sub(SUFFIX.len());
        let mut end = available.min(value.len());
        while end > 0 && !value.is_char_boundary(end) {
            end -= 1;
        }
        let mut output = Vec::with_capacity(max_bytes);
        output.extend_from_slice(value[..end].as_bytes());
        if max_bytes >= SUFFIX.len() {
            output.extend_from_slice(SUFFIX.as_bytes());
        }
        (output, true)
    }

    #[cfg(test)]
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

    #[cfg(test)]
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

    /// Builds the user-facing detail payload from an owned database seed.
    ///
    /// Callers should fetch the seed while holding the database lock and call
    /// this function only after releasing it. Preview generation lives in the
    /// storage-independent preview module.
    pub fn build_history_detail(
        seed: HistoryDetailSeed,
        compact_mode: bool,
    ) -> Result<HistoryDetail> {
        preview::build_history_detail(seed, compact_mode)
    }

    #[cfg(test)]
    fn rich_preview_from_event_data(event_data: &[u8]) -> Vec<StoredPreviewSegment> {
        preview::rich_preview_from_event_data(event_data)
    }

    #[cfg(test)]
    fn rich_preview_segments(event: &Event) -> Vec<StoredPreviewSegment> {
        preview::rich_preview_segments(event)
    }

    #[cfg(test)]
    fn html_preview_from_event_data(event_data: &[u8]) -> Option<String> {
        preview::html_preview_from_event_data(event_data)
    }

    fn compact_text_event(event: &Event) -> Option<Event> {
        classification::compact_text_event(event)
    }

    #[cfg(test)]
    fn find_data<'event>(
        event: &'event Event,
        data_type: &str,
    ) -> Option<&'event copy_event_listener::event::Data> {
        classification::find_data(event, data_type)
    }

    #[cfg(test)]
    fn hash_bytes(value: &[u8]) -> String {
        classification::hash_bytes(value)
    }

    pub fn get_history_page(
        &self,
        cursor: Option<&str>,
        page_size: Option<usize>,
    ) -> Result<HistoryPage> {
        let compact_mode = self.get_compact_mode()?;
        self.get_history_page_for_mode(cursor, page_size, compact_mode)
    }

    fn get_history_page_for_mode(
        &self,
        cursor: Option<&str>,
        page_size: Option<usize>,
        compact_mode: bool,
    ) -> Result<HistoryPage> {
        let page_size = page_size
            .unwrap_or(DEFAULT_HISTORY_PAGE_SIZE)
            .clamp(1, MAX_HISTORY_PAGE_SIZE);
        let fetch_limit = (page_size + 1) as i64;
        let cursor = cursor
            .map(HistoryCursor::decode)
            .transpose()
            .map_err(rusqlite::Error::InvalidParameterName)?;
        let mut items = self.query_history_summaries(cursor.as_ref(), fetch_limit, compact_mode)?;
        let has_more = items.len() > page_size;
        if has_more {
            items.truncate(page_size);
        }
        let next_cursor = has_more.then(|| items.last()).flatten().map(|item| {
            HistoryCursor {
                timestamp: item.timestamp,
                content_hash: item.content_hash.clone(),
            }
            .encode()
        });
        let stats = self.get_history_stats()?;
        let total_count = if compact_mode {
            stats.compact_visible_items
        } else {
            stats.total_items
        };

        Ok(HistoryPage {
            items,
            next_cursor,
            has_more,
            total_count,
            total_bytes: stats.total_bytes,
        })
    }

    fn query_history_summaries(
        &self,
        cursor: Option<&HistoryCursor>,
        limit: i64,
        compact_mode: bool,
    ) -> Result<Vec<HistorySummary>> {
        let alias = if compact_mode {
            "event"
        } else {
            "clipboard_events"
        };
        let data_type = if compact_mode { "'text'" } else { "data_type" };
        let compact_filter = if compact_mode {
            "compact_content_hash IS NOT NULL
             AND NOT EXISTS (
                 SELECT 1
                 FROM clipboard_events AS newer
                 WHERE newer.compact_content_hash = event.compact_content_hash
                   AND (
                       newer.timestamp > event.timestamp
                       OR (
                           newer.timestamp = event.timestamp
                           AND newer.content_hash < event.content_hash
                       )
                   )
             )"
        } else {
            "1 = 1"
        };
        let cursor_filter = if cursor.is_some() {
            "AND (
                timestamp < ?1
                OR (timestamp = ?1 AND content_hash > ?2)
             )"
        } else {
            ""
        };
        let limit_parameter = if cursor.is_some() { "?3" } else { "?1" };
        let from = if compact_mode {
            "clipboard_events AS event"
        } else {
            "clipboard_events"
        };
        let query = format!(
            "SELECT
                content_hash,
                {data_type},
                summary_display,
                summary_truncated,
                timestamp,
                source_bundle_id,
                is_remote_clipboard,
                byte_count
             FROM {from}
             WHERE {compact_filter}
             {cursor_filter}
             ORDER BY {alias}.timestamp DESC, {alias}.content_hash ASC
             LIMIT {limit_parameter}"
        );
        let mut statement = self.conn.prepare(&query)?;
        let map_row = |row: &rusqlite::Row<'_>| {
            let data_type = row.get::<_, String>(1)?;
            Ok(HistorySummary {
                content_hash: row.get(0)?,
                has_detail: Self::data_type_has_detail(&data_type),
                data_type,
                display: row.get(2)?,
                display_truncated: row.get(3)?,
                timestamp: row.get(4)?,
                source_bundle_id: row.get(5)?,
                is_remote_clipboard: row.get(6)?,
                byte_count: row.get::<_, i64>(7)?.max(0) as u64,
            })
        };
        let rows = if let Some(cursor) = cursor {
            statement.query_map(
                params![cursor.timestamp, &cursor.content_hash, limit],
                map_row,
            )?
        } else {
            statement.query_map([limit], map_row)?
        };
        rows.collect()
    }

    fn data_type_has_detail(data_type: &str) -> bool {
        matches!(
            data_type,
            "rtf"
                | "html"
                | "png"
                | "tiff"
                | "tif"
                | "jpeg"
                | "jpg"
                | "gif"
                | "webp"
                | "bmp"
                | "heic"
                | "heif"
                | "video"
        )
    }

    pub fn get_history_detail_seed(&self, content_hash: &str) -> Result<Option<HistoryDetailSeed>> {
        self.conn
            .query_row(
                "SELECT
                    content_hash,
                    event_data,
                    data_type,
                    display,
                    compact_display,
                    timestamp,
                    source_bundle_id,
                    is_remote_clipboard,
                    byte_count
                 FROM clipboard_events
                 WHERE content_hash = ?1",
                [content_hash],
                |row| {
                    Ok(HistoryDetailSeed {
                        content_hash: row.get(0)?,
                        event_data: row.get(1)?,
                        data_type: row.get(2)?,
                        display: row.get(3)?,
                        compact_display: row.get(4)?,
                        timestamp: row.get(5)?,
                        source_bundle_id: row.get(6)?,
                        is_remote_clipboard: row.get(7)?,
                        byte_count: row.get::<_, i64>(8)?.max(0) as u64,
                    })
                },
            )
            .optional()
    }

    pub fn get_restore_seed(&self, content_hash: &str) -> Result<Option<RestoreSeed>> {
        let compact_mode = self.get_compact_mode()?;
        let seed = self
            .conn
            .query_row(
                "SELECT
                    content_hash,
                    compact_content_hash,
                    event_data,
                    source_bundle_id,
                    is_remote_clipboard
                 FROM clipboard_events
                 WHERE content_hash = ?1",
                [content_hash],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, bool>(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            stored_content_hash,
            compact_content_hash,
            event_data,
            source_bundle_id,
            is_remote_clipboard,
        )) = seed
        else {
            return Ok(None);
        };
        let content_hash = if compact_mode {
            let Some(compact_content_hash) = compact_content_hash else {
                return Ok(None);
            };
            compact_content_hash
        } else {
            stored_content_hash
        };

        Ok(Some(RestoreSeed {
            content_hash,
            event_data,
            source_bundle_id,
            is_remote_clipboard,
            compact_mode,
        }))
    }

    pub(crate) fn history_mirror_database_path(&self) -> Result<PathBuf> {
        self.path.clone().ok_or_else(|| {
            rusqlite::Error::InvalidParameterName(
                "history mirror requires an on-disk database".to_string(),
            )
        })
    }

    /// Loads a mirror snapshot through an independent read-only connection.
    ///
    /// The history-mirror worker calls this after debounce, so the application's
    /// shared database mutex is never held while every persisted event BLOB is
    /// cloned. Reading the latest committed database also makes delayed or
    /// reordered refresh signals harmless: a signal never carries stale rows.
    pub(crate) fn visit_history_snapshot_rows_from_path<F>(path: &Path, visitor: F) -> Result<()>
    where
        F: FnMut(crate::history_mirror::HistorySnapshotRow) -> bool,
    {
        crate::private_fs::harden_private_file_if_exists(path)
            .map_err(|_| Self::private_database_error("mirror read validation"))?;
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(std::time::Duration::from_secs(2))?;
        Self::visit_history_snapshot_rows_in(&connection, visitor)
    }

    #[cfg(test)]
    pub fn history_snapshot_rows(&self) -> Result<Vec<crate::history_mirror::HistorySnapshotRow>> {
        Self::history_snapshot_rows_in(&self.conn)
    }

    #[cfg(test)]
    fn history_snapshot_rows_in(
        connection: &Connection,
    ) -> Result<Vec<crate::history_mirror::HistorySnapshotRow>> {
        let mut rows = Vec::new();
        Self::visit_history_snapshot_rows_in(connection, |row| {
            rows.push(row);
            true
        })?;
        Ok(rows)
    }

    fn visit_history_snapshot_rows_in<F>(connection: &Connection, mut visitor: F) -> Result<()>
    where
        F: FnMut(crate::history_mirror::HistorySnapshotRow) -> bool,
    {
        if settings::get_compact_mode(connection)? {
            return Self::visit_compact_history_snapshot_rows_in(connection, visitor);
        }

        let mut statement = connection.prepare(
            "SELECT
                content_hash,
                event_data,
                data_type,
                display,
                timestamp,
                source_bundle_id,
                is_remote_clipboard
             FROM clipboard_events
             ORDER BY timestamp DESC, content_hash ASC",
        )?;
        let mapped = statement.query_map([], |row| {
            Ok(crate::history_mirror::HistorySnapshotRow {
                content_hash: row.get(0)?,
                event_data: row.get(1)?,
                data_type: row.get(2)?,
                display: row.get(3)?,
                timestamp: row.get(4)?,
                source_bundle_id: row.get(5)?,
                is_remote_clipboard: row.get(6)?,
            })
        })?;
        for row in mapped {
            if !visitor(row?) {
                break;
            }
        }
        Ok(())
    }

    fn visit_compact_history_snapshot_rows_in<F>(
        connection: &Connection,
        mut visitor: F,
    ) -> Result<()>
    where
        F: FnMut(crate::history_mirror::HistorySnapshotRow) -> bool,
    {
        let mut statement = connection.prepare(
            "SELECT
                compact_content_hash,
                compact_display,
                timestamp,
                source_bundle_id,
                is_remote_clipboard
             FROM clipboard_events AS event
             WHERE compact_content_hash IS NOT NULL
               AND compact_display IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1
                   FROM clipboard_events AS newer
                   WHERE newer.compact_content_hash = event.compact_content_hash
                     AND (
                         newer.timestamp > event.timestamp
                         OR (
                             newer.timestamp = event.timestamp
                             AND newer.content_hash < event.content_hash
                         )
                     )
               )
             ORDER BY timestamp DESC, content_hash ASC",
        )?;
        let mapped = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })?;

        for mapped_row in mapped {
            let (content_hash, display, timestamp, source_bundle_id, is_remote_clipboard) =
                mapped_row?;
            let mut data_list = vec![Data {
                r#type: "public.utf8-plain-text".to_string(),
                data: display.clone(),
            }];
            if let Some(source_bundle_id) = source_bundle_id.as_ref() {
                data_list.push(Data {
                    r#type: SOURCE_TYPE.to_string(),
                    data: source_bundle_id.as_bytes().to_vec(),
                });
            }
            if is_remote_clipboard {
                data_list.push(Data {
                    r#type: REMOTE_CLIPBOARD_TYPE.to_string(),
                    data: Vec::new(),
                });
            }
            let event_data = encode_event_blob(&Event {
                items: vec![Item { data_list }],
            })
            .map_err(|_| {
                rusqlite::Error::InvalidParameterName(
                    "compact history mirror row could not be encoded".to_string(),
                )
            })?;
            let row = crate::history_mirror::HistorySnapshotRow {
                content_hash,
                event_data,
                data_type: "text".to_string(),
                display,
                timestamp,
                source_bundle_id,
                is_remote_clipboard,
            };
            if !visitor(row) {
                break;
            }
        }
        Ok(())
    }

    pub fn get_tray_events(&self) -> Result<Vec<TrayEvent>> {
        let compact_mode = self.get_compact_mode()?;
        self.get_tray_events_for_mode(compact_mode, TRAY_HISTORY_LIMIT)
    }

    fn get_tray_events_for_mode(
        &self,
        compact_mode: bool,
        requested_limit: usize,
    ) -> Result<Vec<TrayEvent>> {
        let limit = requested_limit.clamp(1, TRAY_HISTORY_LIMIT) as i64;
        let data_type = if compact_mode { "'text'" } else { "data_type" };
        let from = if compact_mode {
            "clipboard_events AS event"
        } else {
            "clipboard_events"
        };
        let compact_filter = if compact_mode {
            "compact_content_hash IS NOT NULL
             AND NOT EXISTS (
                 SELECT 1
                 FROM clipboard_events AS newer
                 WHERE newer.compact_content_hash = event.compact_content_hash
                   AND (
                       newer.timestamp > event.timestamp
                       OR (
                           newer.timestamp = event.timestamp
                           AND newer.content_hash < event.content_hash
                       )
                   )
             )"
        } else {
            "1 = 1"
        };
        let query = format!(
            "SELECT
                content_hash,
                {data_type},
                summary_display
             FROM {from}
             WHERE {compact_filter}
             ORDER BY timestamp DESC, content_hash ASC
             LIMIT ?1"
        );
        let mut statement = self.conn.prepare(&query)?;
        let rows = statement.query_map([limit], |row| {
            Ok(TrayEvent {
                content_hash: row.get(0)?,
                data_type: row.get(1)?,
                display: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_history_stats(&self) -> Result<HistoryStats> {
        let (total_items, total_bytes) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(byte_count), 0) FROM clipboard_events",
            [],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?)),
        )?;
        let compact_visible_items = self.conn.query_row(
            "SELECT COUNT(*)
             FROM clipboard_events AS event
             WHERE compact_content_hash IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1
                   FROM clipboard_events AS newer
                   WHERE newer.compact_content_hash = event.compact_content_hash
                     AND (
                         newer.timestamp > event.timestamp
                         OR (
                             newer.timestamp = event.timestamp
                             AND newer.content_hash < event.content_hash
                         )
                     )
               )",
            [],
            |row| row.get(0),
        )?;
        Ok(HistoryStats {
            total_items,
            total_bytes,
            compact_visible_items,
        })
    }

    #[cfg(test)]
    pub fn get_all_events(&self) -> Result<Vec<StoredEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT content_hash, data_type, display, event_data, timestamp
             FROM clipboard_events
             ORDER BY timestamp DESC, content_hash ASC",
        )?;

        let event_iter = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;

        let mut events = Vec::new();
        let compact_mode = self.get_compact_mode()?;
        let mut compact_hashes = std::collections::HashSet::new();

        for event in event_iter {
            let (content_hash, data_type, display, event_data, timestamp) = event?;
            if compact_mode {
                let Ok(event) = Self::event_from_blob(&event_data) else {
                    continue;
                };
                let Some(compact_event) = Self::compact_text_event(&event) else {
                    continue;
                };
                let Some(classified) = Self::classify_event(&compact_event) else {
                    continue;
                };
                if !compact_hashes.insert(classified.content_hash) {
                    continue;
                }
                events.push(StoredEvent::new(
                    content_hash,
                    classified.data_type,
                    classified.display,
                    None,
                    Vec::new(),
                    timestamp,
                ));
            } else {
                events.push(StoredEvent::new(
                    content_hash,
                    data_type,
                    display,
                    Self::html_preview_from_event_data(&event_data),
                    Self::rich_preview_from_event_data(&event_data),
                    timestamp,
                ));
            }
        }
        Ok(events)
    }

    #[cfg(test)]
    pub fn get_event_by_content_hash(&self, content_hash: &str) -> Result<Option<Event>> {
        let mut stmt = self
            .conn
            .prepare("SELECT event_data FROM clipboard_events WHERE content_hash = ?1")?;

        let mut rows = stmt.query([content_hash])?;
        if let Some(row) = rows.next()? {
            let event_data: Vec<u8> = row.get(0)?;
            let event = Self::event_from_blob(&event_data)?;
            if self.get_compact_mode()? {
                Ok(Self::compact_text_event(&event))
            } else {
                Ok(Some(event))
            }
        } else {
            Ok(None)
        }
    }

    fn event_from_blob(event_data: &[u8]) -> Result<Event> {
        decode_event_blob(event_data)
            .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))
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
        let transaction = self.conn.unchecked_transaction()?;
        Self::cleanup_old_events_in(&transaction)?;
        transaction.commit()
    }

    fn cleanup_old_events_in(connection: &Connection) -> Result<()> {
        let max_items = settings::get_max_items(connection)?;
        let max_history_bytes = settings::get_max_history_bytes(connection)?;
        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM clipboard_events", [], |row| {
                row.get(0)
            })?;

        if count > max_items as i64 {
            let excess = count - max_items as i64;
            connection.execute(
                "DELETE FROM clipboard_events WHERE content_hash IN (
                    SELECT content_hash FROM clipboard_events
                    ORDER BY timestamp ASC, content_hash DESC
                    LIMIT ?1
                )",
                [excess],
            )?;
        }

        let total_bytes: i64 = connection.query_row(
            "SELECT COALESCE(SUM(byte_count), 0) FROM clipboard_events",
            [],
            |row| row.get(0),
        )?;
        let total_bytes = total_bytes.max(0) as u64;
        if total_bytes > max_history_bytes {
            let mut bytes_to_reclaim = total_bytes - max_history_bytes;
            let mut statement = connection.prepare(
                "SELECT content_hash, byte_count
                 FROM clipboard_events
                 ORDER BY timestamp ASC, content_hash DESC",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?.max(0) as u64,
                ))
            })?;
            let mut hashes_to_delete = Vec::new();
            for row in rows {
                let (content_hash, byte_count) = row?;
                hashes_to_delete.push(content_hash);
                bytes_to_reclaim = bytes_to_reclaim.saturating_sub(byte_count);
                if bytes_to_reclaim == 0 {
                    break;
                }
            }
            drop(statement);
            for content_hash in hashes_to_delete {
                connection.execute(
                    "DELETE FROM clipboard_events WHERE content_hash = ?1",
                    [content_hash],
                )?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history_mirror::{HistoryMirror, HistoryMirrorConfig};
    use crate::pasteboard_protocol::{
        prepare_event_for_restore, AUTO_GENERATED_TYPE, CONCEALED_TYPE, LEGACY_TRANSIENT_TYPE,
        ONEPASSWORD_TYPE, PASTEBOARD_GENERATOR_TYPE, TRANSIENT_TYPE, TYPEIT4ME_CLIPPING_TYPE,
    };
    use std::time::Duration;

    const PROTOCOL_SKIP_CASES: [(&str, &str, &[u8]); 7] = [
        ("universal transient", TRANSIENT_TYPE, b""),
        (
            "TextExpander or Butler transient",
            LEGACY_TRANSIENT_TYPE,
            b"synthetic marker payload",
        ),
        ("TypeIt4Me transient", TYPEIT4ME_CLIPPING_TYPE, b""),
        (
            "Typinator transient",
            PASTEBOARD_GENERATOR_TYPE,
            b"synthetic marker payload",
        ),
        (
            "autogenerated",
            AUTO_GENERATED_TYPE,
            b"synthetic marker payload",
        ),
        ("universal concealed", CONCEALED_TYPE, b""),
        (
            "1Password concealed",
            ONEPASSWORD_TYPE,
            b"synthetic marker payload",
        ),
    ];

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

    fn multi_item_event(data_lists: Vec<Vec<Data>>) -> Event {
        Event {
            items: data_lists
                .into_iter()
                .map(|data_list| Item { data_list })
                .collect(),
        }
    }

    fn protocol_skipped_event(label: &str, marker_type: &str, marker_payload: &[u8]) -> Event {
        let body = format!("synthetic skipped {label}");
        multi_item_event(vec![
            vec![
                data("public.utf8-plain-text", body.as_bytes()),
                data(SOURCE_TYPE, b"com.example.synthetic"),
                data(REMOTE_CLIPBOARD_TYPE, b"synthetic remote payload"),
            ],
            vec![data(marker_type, marker_payload)],
        ])
    }

    fn assert_history_downstreams_are_empty(db: &Database, body_hash: &str, case: &str) {
        let page = db
            .get_history_page(None, Some(50))
            .expect("history page should load");
        assert!(page.items.is_empty(), "{case} leaked into history paging");
        assert_eq!(page.total_count, 0, "{case} changed the visible row count");
        assert!(
            db.get_history_detail_seed(body_hash)
                .expect("history detail lookup should succeed")
                .is_none(),
            "{case} leaked into the detail path"
        );
        assert!(
            db.get_tray_events()
                .expect("tray history should load")
                .is_empty(),
            "{case} leaked into the tray path"
        );
        assert!(
            db.get_all_events()
                .expect("legacy history projection should load")
                .is_empty(),
            "{case} leaked into preview generation"
        );
        assert!(
            db.history_snapshot_rows()
                .expect("mirror rows should load")
                .is_empty(),
            "{case} leaked into the JSONL snapshot source"
        );
        let stats = db.get_history_stats().expect("history stats should load");
        assert_eq!(stats.total_items, 0, "{case} was persisted");
        assert_eq!(stats.total_bytes, 0, "{case} consumed retained bytes");
    }

    fn assert_database_mirror_is_empty(database_path: &Path, label: &str) {
        let output = database_path
            .parent()
            .expect("database should have a parent")
            .join(format!("{label}.jsonl"));
        let mirror = HistoryMirror::start_database(
            HistoryMirrorConfig::new(output.clone(), 4_096).with_debounce(Duration::ZERO),
            database_path.to_path_buf(),
        )
        .expect("database-backed mirror should start");
        mirror
            .schedule_refresh()
            .expect("empty mirror refresh should schedule");
        mirror
            .flush(Duration::from_secs(2))
            .expect("empty mirror refresh should flush");
        assert_eq!(
            std::fs::read_to_string(&output).expect("mirror output should be readable"),
            "",
            "{label} leaked into JSONL"
        );
        mirror
            .shutdown(Duration::from_secs(2))
            .expect("mirror should stop");
    }

    fn valid_png(width: u32, height: u32, additional_bytes: usize) -> Vec<u8> {
        let mut png = vec![0; 24 + additional_bytes];
        png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png[12..16].copy_from_slice(b"IHDR");
        png[16..20].copy_from_slice(&width.to_be_bytes());
        png[20..24].copy_from_slice(&height.to_be_bytes());
        png
    }

    fn detail_seed(clipboard_event: &Event) -> HistoryDetailSeed {
        HistoryDetailSeed {
            content_hash: "a".repeat(64),
            event_data: encode_event_blob(clipboard_event).expect("detail event should encode"),
            data_type: "synthetic".to_string(),
            display: b"Synthetic".to_vec(),
            compact_display: None,
            timestamp: 1,
            source_bundle_id: None,
            is_remote_clipboard: false,
            byte_count: 0,
        }
    }

    fn in_memory_database() -> Database {
        let db = Database {
            conn: Connection::open_in_memory().expect("in-memory database should open"),
            path: None,
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

    fn temp_database_path(label: &str) -> PathBuf {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("copy_stack_{label}_{}_{}", std::process::id(), now));
        let data_dir = root.join("data");
        std::fs::create_dir(&root).expect("private test root should be created");
        std::fs::create_dir(&data_dir).expect("private test data directory should be created");
        #[cfg(unix)]
        for directory in [&root, &data_dir] {
            std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
                .expect("test directory should be private");
        }
        data_dir.join("copy_stack.db")
    }

    fn remove_database_files(path: &std::path::Path) {
        if let Some(root) = path.parent().and_then(Path::parent) {
            let _ = std::fs::remove_dir_all(root);
        }
    }

    fn create_version_one_database(path: &std::path::Path, clipboard_event: &Event) {
        let connection = Connection::open(path).expect("legacy database should open");
        connection
            .execute_batch(
                "PRAGMA user_version = 1;
                 CREATE TABLE clipboard_events (
                    content_hash TEXT PRIMARY KEY,
                    event_data BLOB NOT NULL,
                    data_type TEXT NOT NULL,
                    display BLOB NOT NULL,
                    timestamp INTEGER NOT NULL
                 );
                 CREATE TABLE settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                 );",
            )
            .expect("legacy schema should initialize");
        let event_blob = encode_event_blob(clipboard_event).expect("legacy event should encode");
        connection
            .execute(
                "INSERT INTO clipboard_events
                 (content_hash, event_data, data_type, display, timestamp)
                 VALUES (?1, ?2, 'text', ?3, 1)",
                params!["0".repeat(64), event_blob, b"legacy".to_vec()],
            )
            .expect("legacy row should insert");
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
    fn migration_failpoints_roll_back_the_original_on_disk_database() {
        let clipboard_event = event(vec![data("public.utf8-plain-text", b"rollback row")]);
        for failpoint in [
            MigrationFailpoint::AfterCreateReplacement,
            MigrationFailpoint::AfterCopy,
            MigrationFailpoint::AfterValidation,
            MigrationFailpoint::AfterDropOriginal,
        ] {
            let path = temp_database_path("migration_rollback");
            create_version_one_database(&path, &clipboard_event);
            let db = Database {
                conn: Connection::open(&path).expect("database should reopen"),
                path: Some(path.clone()),
            };
            assert!(
                db.initialize_schema_with_failpoint(Some(failpoint))
                    .is_err(),
                "{failpoint:?}"
            );
            drop(db);

            let connection = Connection::open(&path).expect("rolled-back database should reopen");
            assert_eq!(
                schema::user_version(&connection).expect("version should load"),
                1
            );
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM clipboard_events", [], |row| {
                        row.get::<_, u64>(0)
                    })
                    .expect("original table should remain readable"),
                1
            );
            assert!(
                !Database::table_exists_in(&connection, "clipboard_events_next")
                    .expect("replacement table state should load")
            );
            drop(connection);
            remove_database_files(&path);
        }
    }

    #[test]
    fn corrupt_legacy_row_aborts_migration_without_replacing_original_table() {
        let path = temp_database_path("migration_corrupt");
        let connection = Connection::open(&path).expect("database should open");
        connection
            .execute_batch(
                "PRAGMA user_version = 1;
                 CREATE TABLE clipboard_events (
                    content_hash TEXT PRIMARY KEY,
                    event_data BLOB NOT NULL,
                    data_type TEXT NOT NULL,
                    display BLOB NOT NULL,
                    timestamp INTEGER NOT NULL
                 );",
            )
            .expect("legacy schema should initialize");
        connection
            .execute(
                "INSERT INTO clipboard_events
                 (content_hash, event_data, data_type, display, timestamp)
                 VALUES (?1, x'00010203', 'text', x'', 1)",
                ["0".repeat(64)],
            )
            .expect("corrupt fixture should insert");
        drop(connection);

        assert!(Database::open_path(&path).is_err());
        let connection = Connection::open(&path).expect("original database should reopen");
        assert_eq!(
            schema::user_version(&connection).expect("version should load"),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT hex(event_data) FROM clipboard_events", [], |row| {
                    row.get::<_, String>(0)
                })
                .expect("corrupt original row should remain"),
            "00010203"
        );
        drop(connection);
        remove_database_files(&path);
    }

    #[test]
    fn current_schema_startup_is_idempotent_and_does_not_rebuild_history() {
        let path = temp_database_path("migration_idempotent");
        let db = Database::open_path(&path).expect("database should initialize");
        db.insert_event(&event(vec![data(
            "public.utf8-plain-text",
            b"idempotent row",
        )]))
        .expect("event should insert");
        db.conn
            .execute(
                "UPDATE clipboard_events SET summary_display = x'73656E74696E656C'",
                [],
            )
            .expect("sentinel should update");
        let root_page: i64 = db
            .conn
            .query_row(
                "SELECT rootpage FROM sqlite_master
                 WHERE type = 'table' AND name = 'clipboard_events'",
                [],
                |row| row.get(0),
            )
            .expect("root page should load");
        drop(db);

        let reopened = Database::open_path(&path).expect("current database should reopen");
        let reopened_root_page: i64 = reopened
            .conn
            .query_row(
                "SELECT rootpage FROM sqlite_master
                 WHERE type = 'table' AND name = 'clipboard_events'",
                [],
                |row| row.get(0),
            )
            .expect("root page should reload");
        assert_eq!(reopened_root_page, root_page);
        assert_eq!(
            reopened
                .conn
                .query_row("SELECT summary_display FROM clipboard_events", [], |row| {
                    row.get::<_, Vec<u8>>(0)
                })
                .expect("sentinel should remain"),
            b"sentinel"
        );
        assert_eq!(
            schema::user_version(&reopened.conn).expect("version should load"),
            CURRENT_SCHEMA_VERSION
        );
        assert_eq!(
            Database::metadata_version_in(&reopened.conn, CLASSIFIER_METADATA_KEY)
                .expect("classifier version should load"),
            CLASSIFIER_METADATA_VERSION
        );
        drop(reopened);
        remove_database_files(&path);
    }

    #[test]
    fn legacy_json_schema_migrates_metadata_and_versions_transactionally() {
        let path = temp_database_path("migration_legacy_json");
        let connection = Connection::open(&path).expect("legacy database should open");
        connection
            .execute_batch(
                "CREATE TABLE clipboard_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_data TEXT NOT NULL,
                    data_type TEXT NOT NULL,
                    display TEXT NOT NULL,
                    sort_order INTEGER NOT NULL,
                    timestamp TEXT NOT NULL
                 );",
            )
            .expect("legacy JSON schema should initialize");
        let legacy_json = r#"{"items":[{"data_list":[{"type":"public.utf8-plain-text","data":[108,101,103,97,99,121]},{"type":"org.nspasteboard.source","data":[99,111,109,46,101,120,97,109,112,108,101,46,97,112,112]},{"type":"com.apple.is-remote-clipboard","data":[]}]}]}"#;
        connection
            .execute(
                "INSERT INTO clipboard_events
                 (event_data, data_type, display, sort_order, timestamp)
                 VALUES (?1, 'text', 'legacy', 7, '2024-03-09T16:00:00Z')",
                [legacy_json],
            )
            .expect("legacy JSON row should insert");
        drop(connection);

        let db = Database::open_path(&path).expect("legacy JSON database should migrate");
        let page = db
            .get_history_page(None, Some(50))
            .expect("migrated page should load");
        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items[0].source_bundle_id.as_deref(),
            Some("com.example.app")
        );
        assert!(page.items[0].is_remote_clipboard);
        assert_eq!(page.items[0].timestamp, 1_710_000_000_000);
        assert_eq!(
            db.table_columns("clipboard_events")
                .expect("columns should load"),
            REQUIRED_EVENT_COLUMNS
        );
        assert_eq!(
            schema::user_version(&db.conn).expect("version should load"),
            CURRENT_SCHEMA_VERSION
        );
        drop(db);
        remove_database_files(&path);
    }

    #[test]
    fn legacy_sort_order_remains_authoritative_after_cursor_migration() {
        let path = temp_database_path("migration_sort_order");
        let connection = Connection::open(&path).expect("legacy database should open");
        connection
            .execute_batch(
                "CREATE TABLE clipboard_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_data BLOB NOT NULL,
                    data_type TEXT NOT NULL,
                    display BLOB NOT NULL,
                    sort_order INTEGER NOT NULL,
                    timestamp INTEGER NOT NULL
                 );",
            )
            .expect("legacy ordered schema should initialize");

        let high_order = event(vec![data(
            "public.utf8-plain-text",
            b"synthetic high order",
        )]);
        let low_order = event(vec![data("public.utf8-plain-text", b"synthetic low order")]);
        for (clipboard_event, display, sort_order, timestamp) in [
            (
                &high_order,
                b"synthetic high order".as_slice(),
                10_i64,
                1_i64,
            ),
            (&low_order, b"synthetic low order".as_slice(), 5_i64, 9_i64),
        ] {
            connection
                .execute(
                    "INSERT INTO clipboard_events
                     (event_data, data_type, display, sort_order, timestamp)
                     VALUES (?1, 'text', ?2, ?3, ?4)",
                    params![
                        encode_event_blob(clipboard_event).expect("legacy event should encode"),
                        display,
                        sort_order,
                        timestamp,
                    ],
                )
                .expect("legacy ordered row should insert");
        }
        drop(connection);

        let db = Database::open_path(&path).expect("ordered legacy database should migrate");
        let page = db
            .get_history_page(None, Some(50))
            .expect("migrated history should load");
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].display, b"synthetic high order");
        assert_eq!(page.items[1].display, b"synthetic low order");
        assert!(page.items[0].timestamp > page.items[1].timestamp);

        drop(db);
        remove_database_files(&path);
    }

    #[test]
    fn corrupt_timestamp_extremes_return_stable_results_without_panicking() {
        assert_eq!(Database::normalize_unix_timestamp(i64::MIN), i64::MIN);
        assert_eq!(Database::normalize_unix_timestamp(i64::MAX), i64::MAX);

        let db = in_memory_database();
        db.insert_event(&event(vec![data(
            "public.utf8-plain-text",
            b"synthetic timestamp edge",
        )]))
        .expect("fixture should insert");
        db.conn
            .execute("UPDATE clipboard_events SET timestamp = ?1", [i64::MAX])
            .expect("fixture timestamp should update");

        let error = db
            .next_history_timestamp()
            .expect_err("exhausted timestamp range should be rejected");
        assert!(matches!(error, rusqlite::Error::InvalidParameterName(_)));
    }

    #[cfg(unix)]
    #[test]
    fn on_disk_open_prepares_private_database_files_and_redacts_path_failures() {
        use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

        let path = temp_database_path("private_open");
        let db = Database::open_path(&path).expect("private database should open");
        assert_eq!(
            std::fs::symlink_metadata(path.parent().expect("database should have a parent"))
                .expect("private parent metadata should load")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::symlink_metadata(&path)
                .expect("database metadata should load")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::symlink_metadata(&path)
                .expect("database owner should load")
                .uid(),
            std::fs::symlink_metadata(path.parent().expect("database should have a parent"))
                .expect("private parent owner should load")
                .uid()
        );
        drop(db);
        remove_database_files(&path);

        let symlink_path = temp_database_path("private_symlink");
        let target = symlink_path
            .parent()
            .expect("symlink fixture should have a parent")
            .join("target.db");
        std::fs::write(&target, b"not sqlite").expect("symlink target should be created");
        symlink(&target, &symlink_path).expect("database symlink should be created");
        let error = match Database::open_path(&symlink_path) {
            Ok(_) => panic!("database symlinks must be rejected"),
            Err(error) => error.to_string(),
        };
        assert!(!error.contains(&symlink_path.to_string_lossy().into_owned()));
        assert!(!error.contains(&target.to_string_lossy().into_owned()));
        remove_database_files(&symlink_path);
    }

    #[test]
    fn qa_database_override_accepts_only_an_absolute_directory() {
        let absolute = std::env::temp_dir().join("copy-stack-qa-data");
        assert_eq!(
            Database::qa_database_path(&absolute).unwrap(),
            absolute.join(DB_FILE_NAME)
        );
        let error = Database::qa_database_path(Path::new("relative/qa-data"))
            .expect_err("relative QA directory must be rejected");
        assert!(error.to_string().contains("must be absolute"));
        assert!(!error.to_string().contains("relative/qa-data"));
    }

    #[test]
    fn protocol_skips_are_absent_from_every_storage_downstream_in_both_modes() {
        for compact_mode in [false, true] {
            let mode = if compact_mode { "compact" } else { "full" };
            let path = temp_database_path(&format!("protocol_downstream_{mode}"));
            let db = Database::open_path(&path).expect("private database should open");
            db.set_compact_mode(compact_mode)
                .expect("history mode should update");

            for (label, marker_type, marker_payload) in PROTOCOL_SKIP_CASES {
                let clipboard_event = protocol_skipped_event(label, marker_type, marker_payload);
                let body_hash =
                    Database::hash_bytes(format!("synthetic skipped {label}").as_bytes());
                assert!(
                    Database::prepare_history_event(&clipboard_event, compact_mode)
                        .expect("protocol assessment should succeed")
                        .is_none(),
                    "{label} was prepared in {mode} mode"
                );
                assert_eq!(
                    db.event_content_hash(&clipboard_event)
                        .expect("event hash lookup should succeed"),
                    None,
                    "{label} produced a content hash in {mode} mode"
                );
                assert!(
                    !db.insert_event(&clipboard_event)
                        .expect("skipped event insertion should succeed"),
                    "{label} was inserted in {mode} mode"
                );
                assert_history_downstreams_are_empty(
                    &db,
                    &body_hash,
                    &format!("{label} in {mode} mode"),
                );
            }

            assert_database_mirror_is_empty(&path, &format!("protocol-{mode}"));
            drop(db);
            remove_database_files(&path);
        }
    }

    #[test]
    fn protocol_metadata_is_hash_independent_and_round_trips_in_both_modes() {
        for compact_mode in [false, true] {
            let db = in_memory_database();
            db.set_compact_mode(compact_mode)
                .expect("history mode should update");
            let plain = event(vec![data(
                "public.utf8-plain-text",
                b"synthetic protocol identity",
            )]);
            let with_metadata = event(vec![
                data("public.utf8-plain-text", b"synthetic protocol identity"),
                data(SOURCE_TYPE, b"com.example.synthetic"),
                data(REMOTE_CLIPBOARD_TYPE, b"payload does not affect presence"),
            ]);
            let plain_hash = Database::prepare_history_event(&plain, compact_mode)
                .expect("plain event should prepare")
                .expect("plain event should be recordable")
                .content_hash()
                .to_string();
            let metadata_hash = Database::prepare_history_event(&with_metadata, compact_mode)
                .expect("metadata event should prepare")
                .expect("metadata event should be recordable")
                .content_hash()
                .to_string();
            assert_eq!(
                plain_hash, metadata_hash,
                "protocol metadata changed the content hash"
            );

            assert!(db.insert_event(&plain).expect("plain event should insert"));
            assert!(db
                .insert_event(&with_metadata)
                .expect("metadata event should update the same row"));
            let page = db
                .get_history_page(None, Some(50))
                .expect("history should load");
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].content_hash, metadata_hash);
            assert_eq!(
                page.items[0].source_bundle_id.as_deref(),
                Some("com.example.synthetic")
            );
            assert!(page.items[0].is_remote_clipboard);

            let seed = db
                .get_restore_seed(&metadata_hash)
                .expect("restore seed lookup should succeed")
                .expect("restore seed should exist");
            assert_eq!(
                seed.source_bundle_id.as_deref(),
                Some("com.example.synthetic")
            );
            assert!(seed.is_remote_clipboard);
            let restored = prepare_event_for_restore(
                seed.into_event()
                    .expect("restore event should decode")
                    .expect("restore event should exist"),
                Some("com.example.synthetic"),
                true,
            )
            .expect("restore metadata should canonicalize");
            let source_count = restored
                .items
                .iter()
                .flat_map(|item| item.data_list.iter())
                .filter(|data| data.r#type == SOURCE_TYPE)
                .count();
            let remote_count = restored
                .items
                .iter()
                .flat_map(|item| item.data_list.iter())
                .filter(|data| data.r#type == REMOTE_CLIPBOARD_TYPE)
                .count();
            assert_eq!(source_count, 1);
            assert_eq!(remote_count, 1);
        }
    }

    #[test]
    fn legacy_migration_drops_every_protocol_skipped_marker() {
        let path = temp_database_path("protocol_migration");
        let first_event = protocol_skipped_event(
            PROTOCOL_SKIP_CASES[0].0,
            PROTOCOL_SKIP_CASES[0].1,
            PROTOCOL_SKIP_CASES[0].2,
        );
        create_version_one_database(&path, &first_event);
        let connection = Connection::open(&path).expect("legacy database should reopen");
        for (index, (label, marker_type, marker_payload)) in
            PROTOCOL_SKIP_CASES.iter().enumerate().skip(1)
        {
            let clipboard_event = protocol_skipped_event(label, marker_type, marker_payload);
            let event_blob =
                encode_event_blob(&clipboard_event).expect("legacy protocol event should encode");
            connection
                .execute(
                    "INSERT INTO clipboard_events
                     (content_hash, event_data, data_type, display, timestamp)
                     VALUES (?1, ?2, 'text', ?3, ?4)",
                    params![
                        format!("{index:064x}"),
                        event_blob,
                        format!("synthetic skipped {label}").into_bytes(),
                        index as i64 + 1,
                    ],
                )
                .expect("legacy protocol row should insert");
        }
        drop(connection);

        let db = Database::open_path(&path).expect("legacy database should migrate");
        let row_count: u64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM clipboard_events", [], |row| {
                row.get(0)
            })
            .expect("migrated row count should load");
        assert_eq!(row_count, 0);
        for compact_mode in [false, true] {
            db.set_compact_mode(compact_mode)
                .expect("history mode should update");
            for (label, _, _) in PROTOCOL_SKIP_CASES {
                let body_hash =
                    Database::hash_bytes(format!("synthetic skipped {label}").as_bytes());
                assert_history_downstreams_are_empty(&db, &body_hash, &format!("migrated {label}"));
            }
        }
        assert_database_mirror_is_empty(&path, "protocol-migration");
        drop(db);
        remove_database_files(&path);
    }

    #[test]
    fn prepared_compact_events_keep_protocol_metadata_and_reuse_the_effective_hash() {
        let db = in_memory_database();
        let clipboard_event = event(vec![
            data("public.utf8-plain-text", b"compact protocol metadata"),
            data(SOURCE_TYPE, b"com.example.source"),
            data(REMOTE_CLIPBOARD_TYPE, b"marker payload is ignored"),
        ]);
        let prepared = Database::prepare_history_event(&clipboard_event, true)
            .expect("event preparation should succeed")
            .expect("compact text should be accepted");
        let expected_hash = Database::hash_bytes(b"compact protocol metadata");
        assert_eq!(prepared.content_hash(), expected_hash);
        assert!(db
            .insert_prepared_event(prepared)
            .expect("prepared event should insert"));

        let page = db
            .get_history_page_for_mode(None, Some(50), true)
            .expect("compact history should load");
        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items[0].source_bundle_id.as_deref(),
            Some("com.example.source")
        );
        assert!(page.items[0].is_remote_clipboard);
        assert_eq!(page.items[0].data_type, "text");
    }

    #[test]
    fn compact_restore_seed_uses_effective_hash_and_applies_projection_outside_storage() {
        let db = in_memory_database();
        let clipboard_event = event(vec![
            data("public.utf8-plain-text", b"effective restore text"),
            data("public.rtf", br"{\rtf1 effective restore text}"),
            data(SOURCE_TYPE, b"com.example.restore"),
            data(REMOTE_CLIPBOARD_TYPE, b""),
        ]);
        db.insert_event(&clipboard_event)
            .expect("full event should insert");
        let stored_hash = db
            .get_history_page(None, Some(1))
            .expect("history should load")
            .items[0]
            .content_hash
            .clone();
        let effective_hash = Database::hash_bytes(b"effective restore text");
        assert_ne!(stored_hash, effective_hash);

        db.set_compact_mode(true)
            .expect("compact mode should enable");
        let seed = db
            .get_restore_seed(&stored_hash)
            .expect("restore seed lookup should work")
            .expect("restore seed should exist");
        assert_eq!(seed.content_hash, effective_hash);
        assert_eq!(
            seed.source_bundle_id.as_deref(),
            Some("com.example.restore")
        );
        assert!(seed.is_remote_clipboard);

        let restored = seed
            .into_event()
            .expect("restore event should decode")
            .expect("compact projection should exist");
        assert!(Database::find_data(&restored, "public.rtf").is_none());
        assert_eq!(
            Database::find_data(&restored, "public.utf8-plain-text")
                .expect("plain text should remain")
                .data,
            b"effective restore text"
        );
        assert!(Database::find_data(&restored, SOURCE_TYPE).is_some());
        assert!(Database::find_data(&restored, REMOTE_CLIPBOARD_TYPE).is_some());
    }

    #[test]
    fn history_summary_paging_is_bounded_and_uses_stable_cursors() {
        let db = in_memory_database();
        db.set_max_items(1_000).expect("retention should expand");
        for index in 0..120 {
            db.insert_event(&event(vec![data(
                "public.utf8-plain-text",
                format!("summary row {index:03}").as_bytes(),
            )]))
            .expect("fixture should insert");
        }

        let first = db
            .get_history_page(None, None)
            .expect("default page should load");
        assert_eq!(first.items.len(), DEFAULT_HISTORY_PAGE_SIZE);
        assert!(first.has_more);
        assert!(first.next_cursor.is_some());

        let capped = db
            .get_history_page(None, Some(usize::MAX))
            .expect("capped page should load");
        assert_eq!(capped.items.len(), MAX_HISTORY_PAGE_SIZE);

        let mut hashes = Vec::new();
        let mut cursor = None;
        loop {
            let page = db
                .get_history_page(cursor.as_deref(), Some(17))
                .expect("cursor page should load");
            hashes.extend(page.items.iter().map(|item| item.content_hash.clone()));
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
        }
        assert_eq!(hashes.len(), 120);
        assert_eq!(hashes.iter().collect::<HashSet<_>>().len(), 120);
        assert!(db.get_history_page(Some("malformed"), Some(50)).is_err());
    }

    #[test]
    fn history_cursor_tie_breaker_does_not_repeat_or_skip_equal_timestamps() {
        let db = in_memory_database();
        for value in ["same timestamp a", "same timestamp b", "same timestamp c"] {
            db.insert_event(&event(vec![data(
                "public.utf8-plain-text",
                value.as_bytes(),
            )]))
            .expect("fixture should insert");
        }
        db.conn
            .execute("UPDATE clipboard_events SET timestamp = 42", [])
            .expect("timestamps should align");

        let mut hashes = Vec::new();
        let mut cursor = None;
        loop {
            let page = db
                .get_history_page(cursor.as_deref(), Some(1))
                .expect("single-row page should load");
            hashes.extend(page.items.iter().map(|item| item.content_hash.clone()));
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
        }
        let mut sorted = hashes.clone();
        sorted.sort();
        assert_eq!(hashes, sorted);
        assert_eq!(hashes.len(), 3);
    }

    #[test]
    fn compact_projection_deduplicates_effective_text_across_page_boundaries() {
        let db = in_memory_database();
        let rtf = event(vec![
            data("public.utf8-plain-text", b"shared effective text"),
            data("public.rtf", br"{\rtf1 shared effective text}"),
        ]);
        let html = event(vec![
            data("public.utf8-plain-text", b"shared effective text"),
            data("public.html", b"<p>shared effective text</p>"),
        ]);
        db.insert_event(&rtf).expect("RTF should insert");
        db.insert_event(&event(vec![data(
            "public.utf8-plain-text",
            b"distinct compact text",
        )]))
        .expect("plain text should insert");
        db.insert_event(&html).expect("HTML should insert");

        db.set_compact_mode(true)
            .expect("compact mode should enable");
        let mut items = Vec::new();
        let mut cursor = None;
        loop {
            let page = db
                .get_history_page(cursor.as_deref(), Some(1))
                .expect("compact page should load");
            items.extend(page.items);
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
        }

        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|item| item.data_type == "text"));
        assert_eq!(
            items
                .iter()
                .map(|item| String::from_utf8_lossy(&item.display).into_owned())
                .collect::<HashSet<_>>()
                .len(),
            2
        );
        assert_eq!(
            db.get_history_stats()
                .expect("stats should load")
                .compact_visible_items,
            2
        );
    }

    #[test]
    fn summary_and_tray_queries_never_return_unbounded_media_payloads() {
        let db = in_memory_database();
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        png.extend(std::iter::repeat(0x5a).take(32 * 1024));
        db.insert_event(&event(vec![data("public.png", &png)]))
            .expect("PNG should insert");
        db.insert_event(&event(vec![data(
            "public.file-url",
            b"file:///synthetic/does-not-exist/image.png",
        )]))
        .expect("local image URL should insert without reading the file");

        let page = db
            .get_history_page(None, Some(50))
            .expect("summary page should load");
        assert_eq!(page.items.len(), 2);
        assert!(page
            .items
            .iter()
            .all(|item| item.display.len() <= MAX_SUMMARY_DISPLAY_BYTES));
        let png_summary = page
            .items
            .iter()
            .find(|item| item.byte_count > 32 * 1024)
            .expect("PNG summary should exist");
        assert_eq!(png_summary.display, b"PNG");
        assert!(png_summary.display_truncated);

        let seed = db
            .get_history_detail_seed(&png_summary.content_hash)
            .expect("detail seed should load")
            .expect("detail seed should exist");
        assert!(seed.event_data.len() > 32 * 1024);

        let tray = db.get_tray_events().expect("tray snapshot should load");
        assert_eq!(tray.len(), 2);
        assert!(tray
            .iter()
            .all(|item| item.display.len() <= MAX_SUMMARY_DISPLAY_BYTES));
    }

    #[test]
    fn persisted_display_is_bounded_without_truncating_restorable_event_data() {
        let db = in_memory_database();
        let original = vec![b'x'; MAX_DISPLAY_BYTES + 4096];
        let clipboard_event = event(vec![Data {
            r#type: "public.utf8-plain-text".to_string(),
            data: original.clone(),
        }]);
        db.insert_event(&clipboard_event)
            .expect("bounded fixture should insert");

        let stored_display_bytes: i64 = db
            .conn
            .query_row("SELECT length(display) FROM clipboard_events", [], |row| {
                row.get(0)
            })
            .expect("display length should load");
        assert_eq!(stored_display_bytes as usize, MAX_DISPLAY_BYTES);

        let restored = db
            .get_event_by_content_hash(
                &db.event_content_hash(&clipboard_event)
                    .expect("hash should compute")
                    .expect("hash should exist"),
            )
            .expect("stored event should load")
            .expect("stored event should exist");
        assert_eq!(restored.items[0].data_list[0].data, original);
    }

    #[test]
    fn tray_snapshot_uses_a_hard_twenty_item_limit() {
        let db = in_memory_database();
        db.set_max_items(1_000).expect("retention should expand");
        for index in 0..25 {
            db.insert_event(&event(vec![data(
                "public.utf8-plain-text",
                format!("tray row {index:02}").as_bytes(),
            )]))
            .expect("fixture should insert");
        }

        assert_eq!(
            db.get_tray_events()
                .expect("tray snapshot should load")
                .len(),
            TRAY_HISTORY_LIMIT
        );
        assert_eq!(
            db.get_tray_events_for_mode(false, 1)
                .expect("one-row tray snapshot should load")
                .len(),
            1
        );
        assert_eq!(
            db.get_tray_events_for_mode(false, usize::MAX)
                .expect("capped tray snapshot should load")
                .len(),
            TRAY_HISTORY_LIMIT
        );
    }

    #[test]
    fn summaries_only_advertise_details_for_detail_capable_types() {
        let db = in_memory_database();
        db.insert_event(&event(vec![data(
            "public.utf8-plain-text",
            b"plain summary",
        )]))
        .expect("plain text should insert");
        db.insert_event(&event(vec![
            data("public.utf8-plain-text", b"formatted summary"),
            data("public.html", b"<strong>formatted summary</strong>"),
        ]))
        .expect("HTML should insert");

        let page = db
            .get_history_page(None, Some(50))
            .expect("summary page should load");
        assert_eq!(page.total_count, 2);
        assert!(page.total_bytes > 0);
        assert_eq!(
            page.items
                .iter()
                .find(|item| item.data_type == "text")
                .expect("plain summary should exist")
                .has_detail,
            false
        );
        assert!(
            page.items
                .iter()
                .find(|item| item.data_type == "html")
                .expect("HTML summary should exist")
                .has_detail
        );

        db.set_compact_mode(true)
            .expect("compact mode should enable");
        let compact_page = db
            .get_history_page(None, Some(50))
            .expect("compact summary page should load");
        assert!(compact_page.items.iter().all(|item| !item.has_detail));
    }

    #[test]
    fn detail_builder_rejects_png_bombs_and_oversized_html() {
        let bomb = valid_png(100_000, 100_000, 0);
        let bomb_event = event(vec![
            data(
                "public.utf8-plain-text",
                INLINE_ATTACHMENT_PLACEHOLDER.to_string().as_bytes(),
            ),
            data("public.png", &bomb),
        ]);
        let detail = Database::build_history_detail(detail_seed(&bomb_event), false)
            .expect("bomb detail should be safely built");
        assert!(detail.rich_preview.is_empty());

        let oversized_html = vec![b'x'; MAX_HTML_BYTES + 1];
        let html_event = event(vec![
            data("public.utf8-plain-text", b"bounded HTML fallback"),
            data("public.html", &oversized_html),
        ]);
        let detail = Database::build_history_detail(detail_seed(&html_event), false)
            .expect("oversized HTML should be safely omitted");
        assert!(detail.html_preview.is_none());
    }

    #[test]
    fn detail_builder_bounds_local_media_reads_and_rejects_symlinks() {
        let huge_path = temp_png_path();
        let huge_file = File::create(&huge_path).expect("huge preview file should be created");
        huge_file
            .set_len((MAX_PREVIEW_IMAGE_BYTES + 1) as u64)
            .expect("huge preview file should be sparse");
        drop(huge_file);
        let huge_url = format!("file://{}", huge_path.display());
        let huge_event = event(vec![
            data(
                "public.utf8-plain-text",
                INLINE_ATTACHMENT_PLACEHOLDER.to_string().as_bytes(),
            ),
            data("public.file-url", huge_url.as_bytes()),
        ]);
        let detail = Database::build_history_detail(detail_seed(&huge_event), false)
            .expect("huge local preview should be safely omitted");
        assert!(detail.rich_preview.is_empty());
        let _ = std::fs::remove_file(&huge_path);

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let target = temp_png_path();
            std::fs::write(&target, valid_png(32, 32, 8))
                .expect("preview target should be created");
            let link = target.with_extension("linked.png");
            symlink(&target, &link).expect("preview symlink should be created");
            let link_url = format!("file://{}", link.display());
            let link_event = event(vec![
                data(
                    "public.utf8-plain-text",
                    INLINE_ATTACHMENT_PLACEHOLDER.to_string().as_bytes(),
                ),
                data("public.file-url", link_url.as_bytes()),
            ]);
            let detail = Database::build_history_detail(detail_seed(&link_event), false)
                .expect("preview symlink should be safely omitted");
            assert!(detail.rich_preview.is_empty());
            let _ = std::fs::remove_file(&link);
            let _ = std::fs::remove_file(&target);
        }
    }

    #[test]
    fn detail_builder_enforces_segment_and_serialized_ipc_budgets() {
        let placeholder_text = INLINE_ATTACHMENT_PLACEHOLDER
            .to_string()
            .repeat(MAX_PREVIEW_SEGMENTS + 8);
        let png = valid_png(1, 1, 0);
        let mut items = vec![vec![
            data("public.utf8-plain-text", placeholder_text.as_bytes()),
            data("public.png", &png),
        ]];
        items.extend((1..MAX_PREVIEW_SEGMENTS + 8).map(|_| vec![data("public.png", &png)]));
        let segment_event = multi_item_event(items);
        let detail = Database::build_history_detail(detail_seed(&segment_event), false)
            .expect("many-segment detail should build");
        assert_eq!(detail.rich_preview.len(), MAX_PREVIEW_SEGMENTS);

        let mut large_png = valid_png(1920, 1080, 3 * 1024 * 1024);
        large_png[24..].fill(255);
        let large_event = event(vec![
            data(
                "public.utf8-plain-text",
                INLINE_ATTACHMENT_PLACEHOLDER.to_string().as_bytes(),
            ),
            data("public.png", &large_png),
        ]);
        let detail = Database::build_history_detail(detail_seed(&large_event), false)
            .expect("large detail should degrade within the IPC budget");
        assert!(
            serde_json::to_vec(&detail)
                .expect("detail should serialize")
                .len()
                <= MAX_DETAIL_IPC_BYTES
        );
        assert!(detail.rich_preview.is_empty());
    }

    #[test]
    fn compact_detail_builder_never_exposes_html_or_rich_media() {
        let clipboard_event = event(vec![
            data(
                "public.utf8-plain-text",
                INLINE_ATTACHMENT_PLACEHOLDER.to_string().as_bytes(),
            ),
            data("public.html", b"<strong>private formatted detail</strong>"),
            data("public.png", &valid_png(8, 8, 0)),
        ]);
        let detail = Database::build_history_detail(detail_seed(&clipboard_event), true)
            .expect("compact detail should build");
        assert!(detail.html_preview.is_none());
        assert!(detail.rich_preview.is_empty());
    }

    #[test]
    fn byte_budget_retention_removes_oldest_rows_and_stats_match_storage() {
        let db = in_memory_database();
        db.set_max_items(1_000).expect("retention should expand");
        for index in 0..4 {
            db.insert_event(&event(vec![data(
                "public.utf8-plain-text",
                format!("{index}-{}", "x".repeat(300)).as_bytes(),
            )]))
            .expect("fixture should insert");
        }
        let before = db.get_history_stats().expect("stats should load");
        assert_eq!(before.total_items, 4);
        let newest_hash = db
            .get_history_page(None, Some(1))
            .expect("newest row should load")
            .items[0]
            .content_hash
            .clone();

        db.set_max_history_bytes(before.total_bytes / 2)
            .expect("byte budget should update");
        db.cleanup_old_events()
            .expect("byte retention should clean up");
        let after = db.get_history_stats().expect("stats should reload");
        assert!(after.total_items < before.total_items);
        assert!(after.total_bytes <= before.total_bytes / 2);
        assert!(db
            .get_history_detail_seed(&newest_hash)
            .expect("newest lookup should work")
            .is_some());
    }

    #[test]
    fn max_history_bytes_setting_round_trips_and_drives_cleanup_exactly() {
        let db = in_memory_database();
        db.set_max_items(1_000).expect("item limit should expand");
        for value in ["old byte row", "new byte row"] {
            db.insert_event(&event(vec![data(
                "public.utf8-plain-text",
                value.as_bytes(),
            )]))
            .expect("fixture should insert");
        }
        let rows = db
            .history_snapshot_rows()
            .expect("history snapshot should load");
        assert_eq!(rows.len(), 2);
        let newest_hash = rows[0].content_hash.clone();
        let newest_bytes = db
            .get_history_detail_seed(&newest_hash)
            .expect("newest detail lookup should work")
            .expect("newest detail should exist")
            .byte_count;

        db.set_max_history_bytes(newest_bytes)
            .expect("byte limit should persist");
        assert_eq!(
            db.get_max_history_bytes()
                .expect("byte limit should round trip"),
            newest_bytes
        );
        db.cleanup_old_events()
            .expect("stored byte limit should drive cleanup");
        let remaining = db
            .history_snapshot_rows()
            .expect("history snapshot should reload");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].content_hash, newest_hash);

        db.set_max_history_bytes(0)
            .expect("zero-byte limit should persist");
        db.cleanup_old_events()
            .expect("zero-byte limit should clear retained rows");
        assert_eq!(
            db.get_history_stats()
                .expect("history stats should reload")
                .total_items,
            0
        );
    }

    #[test]
    fn app_settings_include_current_history_and_resource_limits() {
        let db = in_memory_database();
        db.insert_event(&event(vec![data(
            "public.utf8-plain-text",
            b"settings history accounting",
        )]))
        .expect("fixture should insert");
        db.set_max_history_bytes(123_456)
            .expect("history limit should update");

        let settings = db.get_settings().expect("settings should load");
        assert_eq!(settings.history_count, 1);
        assert!(settings.history_bytes > 0);
        assert_eq!(settings.history_limit_bytes, 123_456);
        assert_eq!(settings.max_history_bytes, settings.history_limit_bytes);
        assert_eq!(settings.max_event_bytes, MAX_EVENT_BLOB_BYTES as u64);
    }

    #[test]
    fn history_snapshot_rows_are_owned_ordered_and_include_protocol_metadata() {
        let db = in_memory_database();
        for (text, source, remote) in [
            ("snapshot alpha", "com.example.alpha", false),
            ("snapshot beta", "com.example.beta", true),
        ] {
            let mut values = vec![
                data("public.utf8-plain-text", text.as_bytes()),
                data(SOURCE_TYPE, source.as_bytes()),
            ];
            if remote {
                values.push(data(REMOTE_CLIPBOARD_TYPE, b""));
            }
            db.insert_event(&event(values))
                .expect("snapshot fixture should insert");
        }
        db.conn
            .execute("UPDATE clipboard_events SET timestamp = 42", [])
            .expect("snapshot timestamps should align");

        let rows = db
            .history_snapshot_rows()
            .expect("history snapshot should load");
        assert_eq!(rows.len(), 2);
        assert!(rows[0].content_hash < rows[1].content_hash);
        assert!(rows.iter().all(|row| row.timestamp == 42));
        assert!(rows
            .iter()
            .all(|row| !row.event_data.is_empty() && !row.display.is_empty()));
        assert_eq!(
            rows.iter()
                .filter_map(|row| row.source_bundle_id.as_deref())
                .collect::<HashSet<_>>(),
            HashSet::from(["com.example.alpha", "com.example.beta"])
        );
        assert_eq!(rows.iter().filter(|row| row.is_remote_clipboard).count(), 1);
    }

    #[test]
    fn schema_initialization_removes_legacy_source_app_column() {
        let clipboard_event = event(vec![data("public.utf8-plain-text", b"legacy row")]);
        let event_blob = encode_event_blob(&clipboard_event).expect("event should encode");
        let db = Database {
            conn: Connection::open_in_memory().expect("in-memory database should open"),
            path: None,
        };
        db.conn
            .execute_batch(
                "CREATE TABLE clipboard_events (
                    content_hash TEXT PRIMARY KEY,
                    event_data BLOB NOT NULL,
                    data_type TEXT NOT NULL,
                    display BLOB NOT NULL,
                    timestamp INTEGER NOT NULL,
                    source_app TEXT
                );",
            )
            .expect("legacy schema should initialize");
        db.conn
            .execute(
                "INSERT INTO clipboard_events
                 (content_hash, event_data, data_type, display, timestamp, source_app)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                (
                    "legacy-hash",
                    event_blob,
                    "text",
                    b"legacy row".to_vec(),
                    1_i64,
                    "Example App",
                ),
            )
            .expect("legacy row should insert");

        db.initialize_schema()
            .expect("legacy schema should migrate");

        let columns = db
            .table_columns("clipboard_events")
            .expect("columns should load");
        assert!(!columns.iter().any(|column| column == "source_app"));
        assert_eq!(
            db.get_all_events()
                .expect("events should load after migration")
                .len(),
            1
        );
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
    fn classification_prefers_png_over_html() {
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
                    name: String::new(),
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
                    name: String::new(),
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
            data("com.example.source-url", b"https://example.test"),
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
    fn compact_mode_setting_defaults_to_false_and_persists() {
        let db = in_memory_database();

        assert!(!db.get_compact_mode().expect("setting should load"));

        db.set_compact_mode(true).expect("setting should update");

        assert!(db.get_compact_mode().expect("setting should reload"));
        assert!(
            db.get_settings()
                .expect("settings should load")
                .compact_mode
        );
    }

    #[test]
    fn language_setting_defaults_to_system_and_persists() {
        let db = in_memory_database();

        assert_eq!(
            db.get_language().expect("setting should load"),
            LanguagePreference::System
        );

        db.set_language(LanguagePreference::TraditionalChinese)
            .expect("setting should update");

        assert_eq!(
            db.get_language().expect("setting should reload"),
            LanguagePreference::TraditionalChinese
        );
        let settings = db.get_settings().expect("settings should load");
        assert_eq!(settings.language, "zh-TW");
        assert_eq!(settings.resolved_language, "zh-TW");
    }

    #[test]
    fn invalid_stored_language_falls_back_to_system() {
        let db = in_memory_database();
        settings::set(&db.conn, settings::LANGUAGE_KEY, "unsupported")
            .expect("fixture should update");

        assert_eq!(
            db.get_language().expect("setting should load"),
            LanguagePreference::System
        );
    }

    #[test]
    fn compact_mode_stores_formatted_text_as_plain_text() {
        let db = in_memory_database();
        db.set_compact_mode(true)
            .expect("compact mode should enable");
        let formatted_event = event(vec![
            data("public.utf8-plain-text", b"Visible text"),
            data("public.rtf", b"{\\rtf1 Visible text}"),
            data(
                "public.html",
                br#"<p><strong style="color: red">Visible text</strong></p>"#,
            ),
        ]);

        assert!(db
            .insert_event(&formatted_event)
            .expect("formatted text should store"));

        let events = db.get_all_events().expect("events should load");
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].content_hash,
            Database::hash_bytes(b"Visible text")
        );
        assert_eq!(events[0].data_type, "text");
        assert_eq!(events[0].display, b"Visible text");
        assert!(events[0].html_preview.is_none());
        assert!(events[0].rich_preview.is_empty());

        let stored_event = db
            .get_event_by_content_hash(&events[0].content_hash)
            .expect("stored event should load")
            .expect("stored event should exist");
        assert_eq!(stored_event.items.len(), 1);
        assert_eq!(stored_event.items[0].data_list.len(), 1);
        assert_eq!(
            stored_event.items[0].data_list[0].r#type,
            "public.utf8-plain-text"
        );
        assert_eq!(stored_event.items[0].data_list[0].data, b"Visible text");
    }

    #[test]
    fn compact_mode_filters_image_file_and_embedded_media_events() {
        let db = in_memory_database();
        db.set_compact_mode(true)
            .expect("compact mode should enable");
        let mixed_image = event(vec![
            data("public.utf8-plain-text", b"Image label"),
            data("public.png", &[0x89, b'P', b'N', b'G']),
        ]);
        let file = event(vec![
            data("public.utf8-plain-text", b"report.pdf"),
            data("public.file-url", b"file:///tmp/report.pdf"),
        ]);
        let html_with_image = event(vec![
            data("public.utf8-plain-text", b"Caption"),
            data("public.html", br#"<p>Caption</p><img src="photo.png">"#),
        ]);
        let blank_text = event(vec![data("public.utf8-plain-text", b" \n\t")]);
        let inline_attachment = event(vec![data(
            "public.utf8-plain-text",
            "Text \u{fffc}".as_bytes(),
        )]);

        assert!(!db.insert_event(&mixed_image).expect("image should filter"));
        assert!(!db.insert_event(&file).expect("file should filter"));
        assert!(!db
            .insert_event(&html_with_image)
            .expect("embedded image should filter"));
        assert!(!db
            .insert_event(&blank_text)
            .expect("blank text should filter"));
        assert!(!db
            .insert_event(&inline_attachment)
            .expect("inline attachment should filter"));
        assert!(db.get_all_events().expect("events should load").is_empty());
    }

    #[test]
    fn compact_mode_projects_old_formatted_events_without_destroying_them() {
        let db = in_memory_database();
        let formatted_event = event(vec![
            data("public.utf8-plain-text", b"Visible text"),
            data("public.rtf", b"{\\rtf1 Visible text}"),
        ]);
        db.insert_event(&formatted_event)
            .expect("formatted event should store");
        let original_hash = Database::hash_bytes(b"{\\rtf1 Visible text}");

        db.set_compact_mode(true)
            .expect("compact mode should enable");
        let events = db.get_all_events().expect("events should load");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].content_hash, original_hash);
        assert_eq!(events[0].data_type, "text");
        assert_eq!(events[0].display, b"Visible text");

        let compact_restore = db
            .get_event_by_content_hash(&original_hash)
            .expect("event should load")
            .expect("event should remain visible");
        assert_eq!(compact_restore.items[0].data_list.len(), 1);
        assert_eq!(
            db.event_content_hash(&compact_restore)
                .expect("hash should compute"),
            Some(Database::hash_bytes(b"Visible text"))
        );

        db.set_compact_mode(false)
            .expect("compact mode should disable");
        let original_restore = db
            .get_event_by_content_hash(&original_hash)
            .expect("event should load")
            .expect("event should exist");
        assert_eq!(original_restore.items[0].data_list.len(), 2);
    }

    #[test]
    fn compact_mode_deduplicates_old_formats_by_effective_text() {
        let db = in_memory_database();
        let rtf_event = event(vec![
            data("public.utf8-plain-text", b"Same text"),
            data("public.rtf", b"{\\rtf1 Same text}"),
        ]);
        let html_event = event(vec![
            data("public.utf8-plain-text", b"Same text"),
            data("public.html", b"<p>Same text</p>"),
        ]);
        db.insert_event(&rtf_event).expect("RTF should store");
        db.insert_event(&html_event).expect("HTML should store");
        assert_eq!(db.get_all_events().expect("events should load").len(), 2);

        db.set_compact_mode(true)
            .expect("compact mode should enable");
        assert_eq!(db.get_all_events().expect("events should load").len(), 1);

        let plain_event = event(vec![data("public.utf8-plain-text", b"Same text")]);
        assert!(db
            .insert_event(&plain_event)
            .expect("plain text should consolidate old formats"));
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM clipboard_events", [], |row| row
                    .get::<_, i64>(0))
                .expect("row count should load"),
            1
        );
        let events = db.get_all_events().expect("events should load");
        assert_eq!(events[0].content_hash, Database::hash_bytes(b"Same text"));
    }

    #[test]
    fn private_only_events_are_not_classified_or_stored() {
        let db = in_memory_database();
        let event = event(vec![
            data("com.example.private-a", b"alpha"),
            data("com.example.private-b", b"beta"),
            data("com.example.private-c", b"gamma"),
            data("com.example.private-d", b"delta"),
        ]);

        assert!(Database::classify_event(&event).is_none());
        assert_eq!(
            db.event_content_hash(&event)
                .expect("unsupported event hash should be filtered"),
            None
        );
        assert!(!db
            .insert_event(&event)
            .expect("unsupported event should be filtered"));
        assert!(db.get_all_events().expect("events should load").is_empty());
    }

    #[test]
    fn event_blob_preserves_private_metadata_for_classified_events() {
        let event = event(vec![
            data("dyn.agk8", b"metadata"),
            data("com.example.source-url", b"https://example.test"),
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
            "com.example.source-url"
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

        db.insert_event(&event).expect("event should insert");
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
    fn history_jsonl_omits_unsupported_events() {
        let db = in_memory_database();
        let path = temp_jsonl_path();
        let event = event(vec![data("com.example.private", &[0xde, 0xad, 0xbe, 0xef])]);

        assert!(!db
            .insert_event(&event)
            .expect("unsupported event should be filtered"));
        db.write_history_jsonl(&HistoryJsonlConfig {
            path: path.clone(),
            max_data_bytes: 128,
        })
        .expect("JSONL should write");

        let contents = std::fs::read_to_string(&path).expect("JSONL should be readable");
        let _ = std::fs::remove_file(&path);
        assert!(contents.is_empty());
    }

    #[test]
    fn rich_preview_preserves_text_image_text_order() {
        let image_path = temp_png_path();
        let image_bytes = valid_png(16, 16, 3);
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
        let image_bytes = valid_png(16, 16, 3);
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
    fn get_all_events_includes_html_preview_for_formatted_content() {
        let db = in_memory_database();
        let html = br#"<p><strong style="color: red">Visible text</strong></p>"#;
        let event = event(vec![
            data("public.utf8-plain-text", b"Visible text"),
            data("public.rtf", b"{\\rtf1\\b Visible text}"),
            data("public.html", html),
        ]);

        db.insert_event(&event)
            .expect("formatted event should insert");
        let events = db.get_all_events().expect("events should load");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data_type, "rtf");
        assert_eq!(
            events[0].html_preview.as_deref(),
            Some(std::str::from_utf8(html).expect("HTML fixture should be UTF-8"))
        );
    }

    #[test]
    fn get_all_events_includes_rich_preview_segments() {
        let db = in_memory_database();
        let image_path = temp_png_path();
        let image_bytes = valid_png(16, 16, 3);
        std::fs::write(&image_path, &image_bytes).expect("preview image should write");
        let file_url = format!("file://{}", image_path.display());
        let event = event(vec![
            data("public.utf8-plain-text", "文字\n￼".as_bytes()),
            data("public.file-url", file_url.as_bytes()),
        ]);

        db.insert_event(&event).expect("event should insert");
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
    fn metadata_rebuild_removes_legacy_unsupported_rows() {
        let db = in_memory_database();
        let event = event(vec![data("com.example.private", b"opaque payload")]);
        let event_blob = encode_event_blob(&event).expect("private event should encode");

        db.conn
            .execute(
                "INSERT INTO clipboard_events
                 (
                    content_hash,
                    event_data,
                    data_type,
                    display,
                    summary_display,
                    summary_truncated,
                    compact_content_hash,
                    compact_display,
                    source_bundle_id,
                    is_remote_clipboard,
                    byte_count,
                    timestamp,
                    metadata_version
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, NULL, NULL, 0, ?6, ?7, ?8)",
                params![
                    "0".repeat(64),
                    event_blob,
                    "unsupported",
                    b"Unsupported clipboard data".to_vec(),
                    b"Unsupported clipboard data".to_vec(),
                    64_i64,
                    1_i64,
                    CLASSIFIER_METADATA_VERSION,
                ],
            )
            .expect("legacy private row should insert");
        db.rebuild_history_metadata()
            .expect("metadata rebuild should succeed");

        assert!(db.get_all_events().expect("events should load").is_empty());
    }

    #[test]
    fn get_all_events_includes_standalone_file_url_image_preview() {
        let db = in_memory_database();
        let image_path = temp_png_path();
        let image_bytes = valid_png(16, 16, 3);
        std::fs::write(&image_path, &image_bytes).expect("preview image should write");
        let file_url = format!("file://{}", image_path.display());
        let event = event(vec![
            data("public.file-url", file_url.as_bytes()),
            data("public.tiff", &[1, 2, 3]),
        ]);

        db.insert_event(&event).expect("event should insert");
        let events = db.get_all_events().expect("events should load");
        let _ = std::fs::remove_file(&image_path);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data_type, "png");
        assert_eq!(events[0].display, b"PNG");
        assert_eq!(
            events[0].rich_preview,
            vec![StoredPreviewSegment::Image {
                label: image_path
                    .file_name()
                    .expect("image should have a file name")
                    .to_string_lossy()
                    .into_owned(),
                media_type: "image/png".to_string(),
                data: image_bytes,
            }]
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

        db.insert_event(&event).expect("event should insert");
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
            }]
        );
        let serialized =
            serde_json::to_string(&events[0].rich_preview).expect("preview should serialize");
        assert!(!serialized.contains(&video_path.to_string_lossy().into_owned()));
    }
}
