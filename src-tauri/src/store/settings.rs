use crate::i18n::LanguagePreference;
use crate::store::models::DEFAULT_MAX_HISTORY_BYTES;
use rusqlite::{Connection, Result};

pub(super) const DEFAULT_MAX_ITEMS: u32 = 100;
pub(super) const MAX_ITEMS_KEY: &str = "max_items";
pub(super) const MAX_HISTORY_BYTES_KEY: &str = "max_history_bytes";
pub(super) const SHOW_IN_MENU_BAR_KEY: &str = "show_in_menu_bar";
pub(super) const MOVE_RESTORED_ITEM_TO_TOP_KEY: &str = "move_restored_item_to_top";
pub(super) const COMPACT_MODE_KEY: &str = "compact_mode";
pub(super) const LANGUAGE_KEY: &str = "language";

pub(super) fn default_entries() -> [(&'static str, String); 6] {
    [
        (MAX_ITEMS_KEY, DEFAULT_MAX_ITEMS.to_string()),
        (MAX_HISTORY_BYTES_KEY, DEFAULT_MAX_HISTORY_BYTES.to_string()),
        (SHOW_IN_MENU_BAR_KEY, "true".to_string()),
        (MOVE_RESTORED_ITEM_TO_TOP_KEY, "false".to_string()),
        (COMPACT_MODE_KEY, "false".to_string()),
        (LANGUAGE_KEY, "system".to_string()),
    ]
}

pub(super) fn get_max_items(connection: &Connection) -> Result<u32> {
    get_u32(connection, MAX_ITEMS_KEY, DEFAULT_MAX_ITEMS)
}

pub(super) fn set_max_items(connection: &Connection, value: u32) -> Result<()> {
    set(connection, MAX_ITEMS_KEY, &value.to_string())
}

pub(super) fn get_max_history_bytes(connection: &Connection) -> Result<u64> {
    get_u64(connection, MAX_HISTORY_BYTES_KEY, DEFAULT_MAX_HISTORY_BYTES)
}

pub(super) fn set_max_history_bytes(connection: &Connection, value: u64) -> Result<()> {
    set(connection, MAX_HISTORY_BYTES_KEY, &value.to_string())
}

pub(super) fn get_show_in_menu_bar(connection: &Connection) -> Result<bool> {
    get_bool(connection, SHOW_IN_MENU_BAR_KEY, true)
}

pub(super) fn set_show_in_menu_bar(connection: &Connection, value: bool) -> Result<()> {
    set(connection, SHOW_IN_MENU_BAR_KEY, bool_value(value))
}

pub(super) fn get_move_restored_item_to_top(connection: &Connection) -> Result<bool> {
    get_bool(connection, MOVE_RESTORED_ITEM_TO_TOP_KEY, false)
}

pub(super) fn set_move_restored_item_to_top(connection: &Connection, value: bool) -> Result<()> {
    set(connection, MOVE_RESTORED_ITEM_TO_TOP_KEY, bool_value(value))
}

pub(super) fn get_compact_mode(connection: &Connection) -> Result<bool> {
    get_bool(connection, COMPACT_MODE_KEY, false)
}

pub(super) fn set_compact_mode(connection: &Connection, value: bool) -> Result<()> {
    set(connection, COMPACT_MODE_KEY, bool_value(value))
}

pub(super) fn get_language(connection: &Connection) -> Result<LanguagePreference> {
    Ok(get(connection, LANGUAGE_KEY)?
        .as_deref()
        .and_then(LanguagePreference::from_code)
        .unwrap_or(LanguagePreference::System))
}

pub(super) fn set_language(connection: &Connection, value: LanguagePreference) -> Result<()> {
    set(connection, LANGUAGE_KEY, value.code())
}

pub(super) fn get(connection: &Connection, key: &str) -> Result<Option<String>> {
    let mut statement = connection.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = statement.query([key])?;
    rows.next()?.map(|row| row.get(0)).transpose()
}

pub(super) fn set(connection: &Connection, key: &str, value: &str) -> Result<()> {
    connection.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        [key, value],
    )?;
    Ok(())
}

fn get_u32(connection: &Connection, key: &str, default: u32) -> Result<u32> {
    match get(connection, key)? {
        Some(value) => value
            .parse::<u32>()
            .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string())),
        None => Ok(default),
    }
}

fn get_u64(connection: &Connection, key: &str, default: u64) -> Result<u64> {
    match get(connection, key)? {
        Some(value) => value
            .parse::<u64>()
            .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string())),
        None => Ok(default),
    }
}

fn get_bool(connection: &Connection, key: &str, default: bool) -> Result<bool> {
    Ok(match get(connection, key)?.as_deref() {
        Some("false" | "0") => false,
        Some("true" | "1") => true,
        Some(_) | None => default,
    })
}

const fn bool_value(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection() -> Connection {
        let connection = Connection::open_in_memory().expect("settings database should open");
        connection
            .execute(
                "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
                [],
            )
            .expect("settings table should initialize");
        for (key, value) in default_entries() {
            set(&connection, key, &value).expect("default should insert");
        }
        connection
    }

    #[test]
    fn typed_settings_round_trip_and_invalid_values_use_stable_rules() {
        let connection = connection();
        assert_eq!(get_max_items(&connection).unwrap(), DEFAULT_MAX_ITEMS);
        assert!(get_show_in_menu_bar(&connection).unwrap());

        set_max_items(&connection, 321).unwrap();
        set_show_in_menu_bar(&connection, false).unwrap();
        assert_eq!(get_max_items(&connection).unwrap(), 321);
        assert!(!get_show_in_menu_bar(&connection).unwrap());

        set(&connection, SHOW_IN_MENU_BAR_KEY, "invalid").unwrap();
        assert!(get_show_in_menu_bar(&connection).unwrap());
        set(&connection, MAX_ITEMS_KEY, "invalid").unwrap();
        assert!(get_max_items(&connection).is_err());
    }
}
