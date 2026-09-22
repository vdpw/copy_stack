use serde::{Deserialize, Serialize};

pub const DEFAULT_HISTORY_PAGE_SIZE: usize = 50;
pub const MAX_HISTORY_PAGE_SIZE: usize = 100;
pub const MAX_MENU_BAR_ITEM_LIMIT: usize = 1_000;
pub const MAX_SUMMARY_DISPLAY_BYTES: usize = 512;
pub const DEFAULT_MAX_HISTORY_BYTES: u64 = crate::resource_policy::MAX_HISTORY_BYTES;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePreference {
    pub(crate) fn from_code(code: &str) -> Option<Self> {
        match code {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }

    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    pub storage_directory: String,
    pub max_items: u32,
    pub max_history_bytes: u64,
    pub show_in_menu_bar: bool,
    pub menu_bar_item_limit: u32,
    pub move_restored_item_to_top: bool,
    pub compact_mode: bool,
    pub theme: String,
    pub language: String,
    pub resolved_language: String,
    pub history_count: u64,
    pub history_bytes: u64,
    pub history_limit_bytes: u64,
    pub max_event_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistorySummary {
    pub is_pinned: bool,
    pub content_hash: String,
    pub data_type: String,
    pub display: Vec<u8>,
    pub display_truncated: bool,
    pub timestamp: i64,
    pub source_bundle_id: Option<String>,
    pub is_remote_clipboard: bool,
    pub byte_count: u64,
    pub has_detail: bool,
    pub search_preview: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryPage {
    pub items: Vec<HistorySummary>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub total_count: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryDetailSeed {
    pub content_hash: String,
    pub event_data: Vec<u8>,
    pub data_type: String,
    pub display: Vec<u8>,
    pub compact_display: Option<Vec<u8>>,
    pub timestamp: i64,
    pub source_bundle_id: Option<String>,
    pub is_remote_clipboard: bool,
    pub byte_count: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryDetail {
    pub content_hash: String,
    pub html_preview: Option<String>,
    pub text_preview: Option<String>,
    pub rich_preview: Vec<crate::store::StoredPreviewSegment>,
    pub file_items: Vec<FileDetailItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDetailItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub name: String,
    pub path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayEvent {
    pub is_pinned: bool,
    pub content_hash: String,
    pub data_type: String,
    pub display: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayPreview {
    pub data_type: String,
    pub display: Vec<u8>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryStats {
    pub total_items: u64,
    pub total_bytes: u64,
    pub compact_visible_items: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HistoryCursor {
    pub is_pinned: bool,
    pub timestamp: i64,
    pub content_hash: String,
}

impl HistoryCursor {
    const PREFIX: &'static str = "v2";

    pub(crate) fn encode(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            Self::PREFIX,
            u8::from(self.is_pinned),
            self.timestamp,
            self.content_hash
        )
    }

    pub(crate) fn decode(value: &str) -> Result<Self, String> {
        let mut parts = value.splitn(4, ':');
        let prefix = parts.next();
        let is_pinned = match parts.next() {
            Some("0") => false,
            Some("1") => true,
            _ => return Err("history cursor pin state is invalid".to_string()),
        };
        let timestamp = parts.next();
        let content_hash = parts.next();

        if prefix != Some(Self::PREFIX) {
            return Err("unsupported history cursor version".to_string());
        }

        let timestamp = timestamp
            .ok_or_else(|| "history cursor is missing a timestamp".to_string())?
            .parse::<i64>()
            .map_err(|_| "history cursor timestamp is invalid".to_string())?;
        let content_hash =
            content_hash.ok_or_else(|| "history cursor is missing a content hash".to_string())?;
        if content_hash.len() != 64
            || !content_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("history cursor content hash is invalid".to_string());
        }

        Ok(Self {
            is_pinned,
            timestamp,
            content_hash: content_hash.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_cursor_round_trips_and_rejects_malformed_values() {
        let cursor = HistoryCursor {
            is_pinned: true,
            timestamp: 1_725_000_000_123,
            content_hash: "a".repeat(64),
        };
        assert_eq!(
            HistoryCursor::decode(&cursor.encode()).expect("cursor should decode"),
            cursor
        );

        for value in [
            "",
            "v2:1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "v1:nope:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "v1:1:short",
            "v1:1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "v2:2:1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "v2:1:nope:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "v2:0:1:short",
            "v2:0:1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "v2:0:1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:extra",
        ] {
            assert!(HistoryCursor::decode(value).is_err(), "{value}");
        }
    }
}
