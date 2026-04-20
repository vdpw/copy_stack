use chrono::{DateTime, Utc};
use copy_event_listener::event::{Data, Event, Item};
use rusqlite::{Connection, Result};
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use uuid::Uuid;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredEvent {
    pub id: String,
    pub event_data: String,
    pub timestamp: DateTime<Utc>,
}

impl StoredEvent {
    fn new(id: String, event_data: String, timestamp: DateTime<Utc>) -> Self {
        Self {
            id,
            event_data,
            timestamp,
        }
    }
}

struct DbRow {
    id: String,
    event_data: String,
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
        let db_path = std::path::Path::new("copy_stack.db");
        let conn = Connection::open(db_path)?;
        let db = Self { conn };

        db.initialize_schema()?;

        Ok(db)
    }

    fn initialize_schema(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS clipboard_events (
                id TEXT PRIMARY KEY,
                event_data TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                content_hash TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;

        if !self.column_exists("clipboard_events", "content_hash")? {
            self.conn.execute(
                "ALTER TABLE clipboard_events ADD COLUMN content_hash TEXT",
                [],
            )?;
        }

        if !self.column_exists("clipboard_events", "sort_order")? {
            self.conn.execute(
                "ALTER TABLE clipboard_events ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }

        self.conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_events_content_hash
             ON clipboard_events(content_hash)
             WHERE content_hash IS NOT NULL",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_clipboard_events_sort_order
             ON clipboard_events(sort_order DESC, timestamp DESC)",
            [],
        )?;

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

        self.rebuild_history_metadata()?;

        Ok(())
    }

    fn column_exists(&self, table: &str, column: &str) -> Result<bool> {
        let pragma = format!("PRAGMA table_info({})", table);
        let mut stmt = self.conn.prepare(&pragma)?;
        let mut rows = stmt.query([])?;

        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == column {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn rebuild_history_metadata(&self) -> Result<()> {
        let has_sort_order: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM clipboard_events WHERE sort_order > 0",
            [],
            |row| row.get(0),
        )?;
        let order_clause = if has_sort_order > 0 {
            "ORDER BY sort_order DESC, timestamp DESC"
        } else {
            "ORDER BY timestamp DESC"
        };

        let query = format!(
            "SELECT id, event_data FROM clipboard_events {}",
            order_clause
        );
        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| {
            Ok(DbRow {
                id: row.get(0)?,
                event_data: row.get(1)?,
            })
        })?;

        let mut survivors: Vec<(String, String)> = Vec::new();
        let mut seen_hashes = std::collections::HashSet::new();
        let mut duplicate_ids = Vec::new();

        for row in rows {
            let row = row?;
            let content_hash = self.generate_content_hash_from_event_data(&row.event_data)?;

            if seen_hashes.insert(content_hash.clone()) {
                survivors.push((row.id, content_hash));
            } else {
                duplicate_ids.push(row.id);
            }
        }

        for duplicate_id in duplicate_ids {
            self.conn.execute(
                "DELETE FROM clipboard_events WHERE id = ?1",
                [&duplicate_id],
            )?;
        }

        let total = survivors.len() as i64;
        for (index, (id, content_hash)) in survivors.iter().enumerate() {
            let sort_order = total - index as i64;
            self.conn.execute(
                "UPDATE clipboard_events
                 SET content_hash = ?1, sort_order = ?2
                 WHERE id = ?3",
                (content_hash, sort_order, id),
            )?;
        }

        Ok(())
    }

    pub fn get_max_items(&self) -> Result<u32> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM settings WHERE key = 'max_items'")?;
        let mut rows = stmt.query([])?;

        if let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            value
                .parse::<u32>()
                .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))
        } else {
            Ok(100)
        }
    }

    pub fn set_max_items(&self, max_items: u32) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('max_items', ?1)",
            [&max_items.to_string()],
        )?;
        Ok(())
    }

    pub fn insert_event(&self, event: &Event) -> Result<()> {
        let now = Utc::now();
        let event_data = serde_json::to_string(event)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
        let content_hash = self.generate_content_hash(event)?;
        let next_sort_order = self.next_sort_order()?;

        let existing_id = self.find_event_id_by_hash(&content_hash)?;

        if let Some(existing_id) = existing_id {
            self.conn.execute(
                "UPDATE clipboard_events
                 SET event_data = ?1, timestamp = ?2, sort_order = ?3
                 WHERE id = ?4",
                (
                    &event_data,
                    &now.to_rfc3339(),
                    next_sort_order,
                    &existing_id,
                ),
            )?;
            return Ok(());
        }

        self.conn.execute(
            "INSERT INTO clipboard_events (id, event_data, timestamp, content_hash, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                Uuid::new_v4().to_string(),
                event_data,
                now.to_rfc3339(),
                content_hash,
                next_sort_order,
            ),
        )?;

        self.cleanup_old_events()?;

        Ok(())
    }

    pub fn move_event_to_top(&self, id: &str) -> Result<()> {
        let updated = self.conn.execute(
            "UPDATE clipboard_events
             SET sort_order = ?1, timestamp = ?2
             WHERE id = ?3",
            (self.next_sort_order()?, Utc::now().to_rfc3339(), id),
        )?;

        if updated == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        Ok(())
    }

    fn next_sort_order(&self) -> Result<i64> {
        self.conn.query_row(
            "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM clipboard_events",
            [],
            |row| row.get(0),
        )
    }

    fn find_event_id_by_hash(&self, content_hash: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM clipboard_events WHERE content_hash = ?1")?;
        let mut rows = stmt.query([content_hash])?;

        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    fn generate_content_hash_from_event_data(&self, event_data: &str) -> Result<String> {
        match serde_json::from_str::<Event>(event_data) {
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
            let fallback = serde_json::to_string(event)
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
            "SELECT id, event_data, timestamp
             FROM clipboard_events
             ORDER BY sort_order DESC, timestamp DESC",
        )?;

        let event_iter = stmt.query_map([], |row| {
            let timestamp_str: String = row.get(2)?;
            let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp_str)
                .unwrap()
                .with_timezone(&chrono::Utc);

            Ok(StoredEvent::new(row.get(0)?, row.get(1)?, timestamp))
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
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM clipboard_events", [], |row| {
                    row.get(0)
                })?;

        if count > max_items as i64 {
            let excess = count - max_items as i64;
            self.conn.execute(
                "DELETE FROM clipboard_events WHERE id IN (
                    SELECT id FROM clipboard_events
                    ORDER BY sort_order ASC, timestamp ASC
                    LIMIT ?1
                )",
                [excess],
            )?;
        }

        Ok(())
    }
}
