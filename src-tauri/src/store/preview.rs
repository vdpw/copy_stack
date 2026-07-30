//! Bounded, display-only History detail generation.
//!
//! This module depends on pure classification helpers, never on SQLite. Callers
//! must copy a `HistoryDetailSeed` while holding the database lock and build the
//! preview only after releasing it.

use crate::event::decode_event_blob;
use crate::resource_policy::{
    allow_image_preview, MAX_DETAIL_IPC_BYTES, MAX_DISPLAY_BYTES, MAX_HTML_BYTES,
    MAX_PREVIEW_IMAGE_BYTES, MAX_PREVIEW_SEGMENTS,
};
use crate::store::classification::{
    file_url_display_name, file_url_extension, file_url_path, find_data, find_data_in_item,
    find_raw_utf8_display,
};
use crate::store::models::{HistoryDetail, HistoryDetailSeed};
use copy_event_listener::event::{Event, Item};
use rusqlite::Result;
use std::fs::File;
use std::io::Read;
use std::path::Path;

const INLINE_ATTACHMENT_PLACEHOLDER: char = '\u{fffc}';

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
    Video { label: String, media_type: String },
}

pub(super) fn build_history_detail(
    seed: HistoryDetailSeed,
    compact_mode: bool,
) -> Result<HistoryDetail> {
    if seed.content_hash.len() > 128 {
        return Err(rusqlite::Error::InvalidParameterName(
            "history detail identifier is invalid".to_string(),
        ));
    }

    let mut detail = HistoryDetail {
        content_hash: seed.content_hash,
        html_preview: None,
        text_preview: None,
        rich_preview: Vec::new(),
    };
    if compact_mode {
        return Ok(detail);
    }

    let event = event_from_blob(&seed.event_data)?;
    detail.html_preview = bounded_html_preview(&event);
    if find_data(&event, "public.html").is_some() && detail.html_preview.is_none() {
        detail.text_preview = bounded_text_preview(&event);
    }

    for segment in preview_segments_from_event(&event)
        .into_iter()
        .take(MAX_PREVIEW_SEGMENTS)
    {
        detail.rich_preview.push(segment);
        if !history_detail_fits_ipc_budget(&detail)? {
            detail.rich_preview.pop();
        }
    }

    if !history_detail_fits_ipc_budget(&detail)? {
        detail.html_preview = None;
    }
    if !history_detail_fits_ipc_budget(&detail)? {
        detail.text_preview = None;
    }
    if !history_detail_fits_ipc_budget(&detail)? {
        return Err(rusqlite::Error::InvalidParameterName(
            "history detail exceeds the configured response limit".to_string(),
        ));
    }
    Ok(detail)
}

#[cfg(test)]
pub(super) fn rich_preview_from_event_data(event_data: &[u8]) -> Vec<StoredPreviewSegment> {
    let Ok(event) = event_from_blob(event_data) else {
        return Vec::new();
    };
    preview_segments_from_event(&event)
}

#[cfg(test)]
pub(super) fn html_preview_from_event_data(event_data: &[u8]) -> Option<String> {
    let event = event_from_blob(event_data).ok()?;
    bounded_html_preview(&event)
}

fn event_from_blob(event_data: &[u8]) -> Result<Event> {
    decode_event_blob(event_data)
        .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))
}

fn history_detail_fits_ipc_budget(detail: &HistoryDetail) -> Result<bool> {
    serde_json::to_vec(detail)
        .map(|payload| payload.len() <= MAX_DETAIL_IPC_BYTES)
        .map_err(|_| {
            rusqlite::Error::InvalidParameterName(
                "history detail could not be serialized".to_string(),
            )
        })
}

fn bounded_html_preview(event: &Event) -> Option<String> {
    let html = find_data(event, "public.html")?;
    if html.data.len() > MAX_HTML_BYTES {
        return None;
    }
    let html = String::from_utf8_lossy(&html.data)
        .trim_matches('\0')
        .trim()
        .to_string();
    (!html.is_empty()).then_some(html)
}

fn bounded_text_preview(event: &Event) -> Option<String> {
    let text = find_raw_utf8_display(event)?;
    let suffix = "\n…";
    let content_limit = MAX_DISPLAY_BYTES.saturating_sub(suffix.len());
    let mut preview = String::with_capacity(text.len().min(MAX_DISPLAY_BYTES));
    let mut truncated = false;

    for character in text.chars().filter(|character| *character != '\0') {
        if preview.len().saturating_add(character.len_utf8()) > content_limit {
            truncated = true;
            break;
        }
        preview.push(character);
    }

    let mut preview = preview.trim().to_string();
    if preview.is_empty() {
        return None;
    }
    if truncated {
        preview.push_str(suffix);
    }
    Some(preview)
}

fn preview_segments_from_event(event: &Event) -> Vec<StoredPreviewSegment> {
    let rich_preview = rich_preview_segments(event);
    if !rich_preview.is_empty() {
        return rich_preview;
    }

    let image_preview = standalone_image_preview_segments(event);
    if !image_preview.is_empty() {
        return image_preview;
    }

    video_preview_segments(event)
}

pub(super) fn rich_preview_segments(event: &Event) -> Vec<StoredPreviewSegment> {
    let Some(text_template) = find_raw_utf8_display(event) else {
        return Vec::new();
    };
    if !text_template.contains(INLINE_ATTACHMENT_PLACEHOLDER) {
        return Vec::new();
    }

    let mut images = event
        .items
        .iter()
        .filter_map(rich_preview_image_in_item)
        .peekable();
    if images.peek().is_none() {
        return Vec::new();
    }

    let mut segments = Vec::new();
    let mut text_buffer = String::new();

    for character in text_template.chars() {
        if character == INLINE_ATTACHMENT_PLACEHOLDER {
            let remaining =
                MAX_DETAIL_IPC_BYTES.saturating_sub(preview_segments_raw_bytes(&segments));
            push_rich_preview_text(&mut segments, &mut text_buffer, remaining);
            if segments.len() >= MAX_PREVIEW_SEGMENTS {
                break;
            }
            if let Some(image) = images.next() {
                let current_bytes = preview_segments_raw_bytes(&segments);
                if current_bytes.saturating_add(preview_segment_raw_bytes(&image))
                    > MAX_DETAIL_IPC_BYTES
                {
                    break;
                }
                segments.push(image);
            }
        } else {
            if text_buffer.len() >= MAX_DETAIL_IPC_BYTES {
                break;
            }
            text_buffer.push(character);
        }
    }

    if segments.len() < MAX_PREVIEW_SEGMENTS {
        let remaining = MAX_DETAIL_IPC_BYTES.saturating_sub(preview_segments_raw_bytes(&segments));
        push_rich_preview_text(&mut segments, &mut text_buffer, remaining);
    }
    segments.truncate(MAX_PREVIEW_SEGMENTS);
    segments
}

fn push_rich_preview_text(
    segments: &mut Vec<StoredPreviewSegment>,
    text: &mut String,
    max_bytes: usize,
) {
    let mut cleaned = String::with_capacity(text.len().min(max_bytes));
    for character in text.chars().filter(|character| *character != '\0') {
        if cleaned.len().saturating_add(character.len_utf8()) > max_bytes {
            break;
        }
        cleaned.push(character);
    }
    text.clear();
    let cleaned = cleaned.trim().to_string();

    if !cleaned.is_empty() {
        segments.push(StoredPreviewSegment::Text { text: cleaned });
    }
}

fn preview_segments_raw_bytes(segments: &[StoredPreviewSegment]) -> usize {
    segments.iter().fold(0_usize, |total, segment| {
        total.saturating_add(preview_segment_raw_bytes(segment))
    })
}

fn preview_segment_raw_bytes(segment: &StoredPreviewSegment) -> usize {
    match segment {
        StoredPreviewSegment::Text { text } => text.len(),
        StoredPreviewSegment::Image {
            label,
            media_type,
            data,
        } => label
            .len()
            .saturating_add(media_type.len())
            .saturating_add(data.len()),
        StoredPreviewSegment::Video { label, media_type } => {
            label.len().saturating_add(media_type.len())
        }
    }
}

fn rich_preview_image_in_item(item: &Item) -> Option<StoredPreviewSegment> {
    if let Some(data) = find_data_in_item(item, "public.png") {
        if !allow_image_preview(&data.data, "image/png") {
            return None;
        }
        return Some(StoredPreviewSegment::Image {
            label: "Image".to_string(),
            media_type: "image/png".to_string(),
            data: data.data.clone(),
        });
    }

    let file_url = find_data_in_item(item, "public.file-url")?;
    let file_url = String::from_utf8_lossy(&file_url.data);
    let extension = file_url_extension(&file_url)?;
    let media_type = preview_image_media_type(&extension)?;
    let path = file_url_path(&file_url)?;
    let image_data = read_bounded_preview_image(&path, media_type)?;
    let label = file_url_display_name(&file_url).unwrap_or_else(|| "Image".to_string());

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

fn standalone_image_preview_segments(event: &Event) -> Vec<StoredPreviewSegment> {
    if event.items.len() != 1 {
        return Vec::new();
    }
    rich_preview_image_in_item(&event.items[0])
        .into_iter()
        .collect()
}

fn video_preview_segments(event: &Event) -> Vec<StoredPreviewSegment> {
    if event.items.len() != 1 {
        return Vec::new();
    }
    video_preview_in_item(&event.items[0]).into_iter().collect()
}

fn video_preview_in_item(item: &Item) -> Option<StoredPreviewSegment> {
    let file_url = find_data_in_item(item, "public.file-url")?;
    let file_url = String::from_utf8_lossy(&file_url.data);
    let extension = file_url_extension(&file_url)?;
    let media_type = preview_video_media_type(&extension)?;
    let path = file_url_path(&file_url)?;
    if !is_ordinary_regular_file(&path) {
        return None;
    }

    let label = file_url_display_name(&file_url).unwrap_or_else(|| "Video".to_string());
    Some(StoredPreviewSegment::Video {
        label,
        media_type: media_type.to_string(),
    })
}

fn read_bounded_preview_image(path: &Path, media_type: &str) -> Option<Vec<u8>> {
    if !path.is_absolute() {
        return None;
    }
    let before = std::fs::symlink_metadata(path).ok()?;
    if !before.file_type().is_file() || before.len() > MAX_PREVIEW_IMAGE_BYTES as u64 {
        return None;
    }

    let mut file = File::open(path).ok()?;
    let opened = file.metadata().ok()?;
    if !opened.is_file()
        || opened.len() > MAX_PREVIEW_IMAGE_BYTES as u64
        || !same_file_identity(&before, &opened)
    {
        return None;
    }

    let mut bytes = Vec::with_capacity(opened.len() as usize);
    Read::by_ref(&mut file)
        .take((MAX_PREVIEW_IMAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_PREVIEW_IMAGE_BYTES {
        return None;
    }

    let after_open = file.metadata().ok()?;
    let after_path = std::fs::symlink_metadata(path).ok()?;
    if !after_path.file_type().is_file()
        || !same_file_identity(&opened, &after_open)
        || !same_file_identity(&opened, &after_path)
    {
        return None;
    }

    allow_image_preview(&bytes, media_type).then_some(bytes)
}

fn is_ordinary_regular_file(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let Ok(path_metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !path_metadata.file_type().is_file() {
        return false;
    }
    let Ok(file) = File::open(path) else {
        return false;
    };
    let Ok(opened_metadata) = file.metadata() else {
        return false;
    };
    opened_metadata.is_file() && same_file_identity(&path_metadata, &opened_metadata)
}

#[cfg(unix)]
fn same_file_identity(first: &std::fs::Metadata, second: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    first.dev() == second.dev() && first.ino() == second.ino()
}

#[cfg(not(unix))]
fn same_file_identity(_first: &std::fs::Metadata, _second: &std::fs::Metadata) -> bool {
    true
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::encode_event_blob;
    use copy_event_listener::event::Data;

    fn event(data_list: Vec<Data>) -> Event {
        Event {
            items: vec![Item { data_list }],
        }
    }

    #[test]
    fn html_preview_is_trimmed_and_bounded() {
        let html_event = event(vec![Data {
            r#type: "public.html".to_string(),
            data: b"\0 <b>safe</b> \0".to_vec(),
        }]);
        assert_eq!(
            bounded_html_preview(&html_event).as_deref(),
            Some("<b>safe</b>")
        );

        let oversized = event(vec![Data {
            r#type: "public.html".to_string(),
            data: vec![b'x'; MAX_HTML_BYTES + 1],
        }]);
        assert!(bounded_html_preview(&oversized).is_none());
    }

    #[test]
    fn text_preview_preserves_lines_and_stays_bounded() {
        let source = format!(
            "package main\n\nfunc main() {{}}\n{}",
            "x".repeat(MAX_DISPLAY_BYTES)
        );
        let source_event = event(vec![Data {
            r#type: "public.utf8-plain-text".to_string(),
            data: source.into_bytes(),
        }]);
        let preview = bounded_text_preview(&source_event).expect("text preview should exist");

        assert!(preview.starts_with("package main\n\nfunc main() {}"));
        assert!(preview.ends_with("\n…"));
        assert!(preview.len() <= MAX_DISPLAY_BYTES);
    }

    #[test]
    fn inline_preview_preserves_text_image_text_order() {
        let mut png = vec![0_u8; 24];
        png[..8].copy_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        png[12..16].copy_from_slice(b"IHDR");
        png[16..20].copy_from_slice(&1_u32.to_be_bytes());
        png[20..24].copy_from_slice(&1_u32.to_be_bytes());
        let event = Event {
            items: vec![
                Item {
                    data_list: vec![Data {
                        r#type: "public.utf8-plain-text".to_string(),
                        data: format!("before{INLINE_ATTACHMENT_PLACEHOLDER}after").into_bytes(),
                    }],
                },
                Item {
                    data_list: vec![Data {
                        r#type: "public.png".to_string(),
                        data: png,
                    }],
                },
            ],
        };
        let segments = rich_preview_from_event_data(&encode_event_blob(&event).unwrap());
        assert!(matches!(
            segments.as_slice(),
            [
                StoredPreviewSegment::Text { text: before },
                StoredPreviewSegment::Image { .. },
                StoredPreviewSegment::Text { text: after }
            ] if before == "before" && after == "after"
        ));
    }
}
