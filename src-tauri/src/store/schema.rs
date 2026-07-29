use rusqlite::{Connection, Result, Transaction};

pub(crate) const CURRENT_SCHEMA_VERSION: i64 = 2;
pub(crate) const CLASSIFIER_METADATA_VERSION: i64 = 1;
pub(crate) const CLASSIFIER_METADATA_KEY: &str = "classifier_metadata_version";

pub(crate) const REQUIRED_EVENT_COLUMNS: [&str; 13] = [
    "content_hash",
    "event_data",
    "data_type",
    "display",
    "summary_display",
    "summary_truncated",
    "compact_content_hash",
    "compact_display",
    "source_bundle_id",
    "is_remote_clipboard",
    "byte_count",
    "timestamp",
    "metadata_version",
];

pub(crate) fn user_version(connection: &Connection) -> Result<i64> {
    connection.query_row("PRAGMA user_version", [], |row| row.get(0))
}

pub(crate) fn set_user_version(transaction: &Transaction<'_>, version: i64) -> Result<()> {
    transaction.pragma_update(None, "user_version", version)
}

pub(crate) fn create_settings_table(connection: &Connection) -> Result<()> {
    connection.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )?;
    Ok(())
}

pub(crate) fn create_metadata_table(connection: &Connection) -> Result<()> {
    connection.execute(
        "CREATE TABLE IF NOT EXISTS app_metadata (
            key TEXT PRIMARY KEY,
            value INTEGER NOT NULL
        )",
        [],
    )?;
    Ok(())
}

pub(crate) fn create_clipboard_events_table(connection: &Connection, table: &str) -> Result<()> {
    connection.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS {table} (
                content_hash TEXT PRIMARY KEY,
                event_data BLOB NOT NULL,
                data_type TEXT NOT NULL,
                display BLOB NOT NULL,
                summary_display BLOB NOT NULL,
                summary_truncated INTEGER NOT NULL,
                compact_content_hash TEXT,
                compact_display BLOB,
                source_bundle_id TEXT,
                is_remote_clipboard INTEGER NOT NULL,
                byte_count INTEGER NOT NULL,
                timestamp INTEGER NOT NULL,
                metadata_version INTEGER NOT NULL
            )"
        ),
        [],
    )?;
    Ok(())
}

pub(crate) fn drop_clipboard_event_indexes(connection: &Connection) -> Result<()> {
    for index in [
        "idx_clipboard_events_content_hash",
        "idx_clipboard_events_sort_order",
        "idx_clipboard_events_timestamp",
        "idx_clipboard_events_compact",
    ] {
        connection.execute(&format!("DROP INDEX IF EXISTS {index}"), [])?;
    }
    Ok(())
}

pub(crate) fn create_clipboard_event_indexes(connection: &Connection) -> Result<()> {
    connection.execute(
        "CREATE INDEX IF NOT EXISTS idx_clipboard_events_timestamp
         ON clipboard_events(timestamp DESC, content_hash ASC)",
        [],
    )?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS idx_clipboard_events_compact
         ON clipboard_events(
             compact_content_hash,
             timestamp DESC,
             content_hash ASC
         )
         WHERE compact_content_hash IS NOT NULL",
        [],
    )?;
    Ok(())
}
