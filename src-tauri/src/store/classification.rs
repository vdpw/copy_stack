//! Pure clipboard-event classification and compact-mode projection.
//!
//! This module intentionally has no SQLite or filesystem dependencies. Storage
//! owns persistence and delegates representation selection to these functions.

use crate::pasteboard_protocol::{REMOTE_CLIPBOARD_TYPE, SOURCE_TYPE};
use copy_event_listener::event::{Data, Event, Item};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub(super) const FILE_DISPLAY_FORMAT: &str = "copy_stack.file-items.v1";
const INLINE_ATTACHMENT_PLACEHOLDER: char = '\u{fffc}';

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ClassifiedEvent {
    pub(super) content_hash: String,
    pub(super) data_type: String,
    pub(super) display: Vec<u8>,
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

pub(super) fn classify_event(event: &Event) -> Option<ClassifiedEvent> {
    if let Some(data) = find_data(event, "public.rtf") {
        return Some(classified_from_single_data(
            "rtf",
            &data.data,
            display_bytes(find_utf8_display(event).unwrap_or_else(|| "RTF".to_string())),
        ));
    }

    if let Some(data) = find_data(event, "public.png") {
        return Some(classified_from_single_data(
            "png",
            &data.data,
            data.data.clone(),
        ));
    }

    if let Some(data) = find_data(event, "public.html") {
        return Some(classified_from_single_data(
            "html",
            &data.data,
            display_bytes(find_utf8_display(event).unwrap_or_else(|| "HTML".to_string())),
        ));
    }

    if event.items.len() > 1 {
        if let Some(file_urls) = extract_multi_file_urls(event) {
            let data_type = multi_file_url_data_type(&file_urls);
            let display_names = file_display_names(event);
            let display_items = event
                .items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| {
                    file_display_item(item, display_names.get(index).cloned())
                })
                .collect::<Vec<_>>();
            let mut hasher = Sha256::new();
            for file_url in &file_urls {
                hasher.update(file_url);
            }

            return Some(ClassifiedEvent {
                content_hash: format!("{:x}", hasher.finalize()),
                data_type: data_type.to_string(),
                display: file_display_bytes(display_items),
            });
        }
    }

    if event.items.len() == 1 {
        if let Some(data) = find_data_in_item(&event.items[0], "public.file-url") {
            if is_video_file_url(data) {
                let file_url = String::from_utf8_lossy(&data.data);
                return Some(classified_from_single_data(
                    "video",
                    &data.data,
                    display_bytes(
                        file_url_display_name(&file_url).unwrap_or_else(|| "Video".to_string()),
                    ),
                ));
            }

            if let Some(image_type) = image_file_url_type(&event.items[0], data) {
                return Some(classified_from_single_data(
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
            let display_name = file_display_names(event).into_iter().next();

            return Some(classified_from_single_data(
                data_type,
                &data.data,
                file_display_bytes(vec![file_display_item_for_url(
                    data_type,
                    &file_url,
                    display_name,
                )]),
            ));
        }
    }

    classify_plain_utf8_text(event)
}

pub fn parse_file_display(display: &[u8]) -> Option<FileDisplay> {
    serde_json::from_slice::<FileDisplay>(display)
        .ok()
        .filter(|display| display.format == FILE_DISPLAY_FORMAT)
}

pub(super) fn compact_text_event(event: &Event) -> Option<Event> {
    if event.items.len() != 1 || event_contains_attachment(event) {
        return None;
    }

    let text_data = find_data_in_item(&event.items[0], "public.utf8-plain-text")?;
    let text = std::str::from_utf8(&text_data.data).ok()?;
    if text.contains(INLINE_ATTACHMENT_PLACEHOLDER) {
        return None;
    }

    let text = text.chars().filter(|ch| *ch != '\0').collect::<String>();
    if text.trim().is_empty() {
        return None;
    }

    let mut data_list = vec![Data {
        r#type: "public.utf8-plain-text".to_string(),
        data: text.into_bytes(),
    }];
    if let Some(source) = find_data(event, SOURCE_TYPE) {
        data_list.push(source.clone());
    }
    if let Some(remote) = find_data(event, REMOTE_CLIPBOARD_TYPE) {
        data_list.push(remote.clone());
    }

    Some(Event {
        items: vec![Item { data_list }],
    })
}

pub(super) fn label_for_data_type(data_type: &str) -> String {
    match data_type {
        "files" => "Files".to_string(),
        "folders" => "Folders".to_string(),
        "files and folders" => "Files and folders".to_string(),
        _ => data_type.to_string(),
    }
}

pub(super) fn find_data<'event>(event: &'event Event, data_type: &str) -> Option<&'event Data> {
    event
        .items
        .iter()
        .find_map(|item| find_data_in_item(item, data_type))
}

pub(super) fn find_data_in_item<'item>(item: &'item Item, data_type: &str) -> Option<&'item Data> {
    item.data_list.iter().find(|data| data.r#type == data_type)
}

pub(super) fn find_raw_utf8_display(event: &Event) -> Option<String> {
    event.items.iter().find_map(find_raw_utf8_display_in_item)
}

pub(super) fn file_url_extension(file_url: &str) -> Option<String> {
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

pub(super) fn file_url_display_name(file_url: &str) -> Option<String> {
    let path = file_url
        .split(['?', '#'])
        .next()
        .unwrap_or(file_url)
        .trim_end_matches('/');
    let path = path.strip_prefix("file://").unwrap_or(path);
    path_display_name(&percent_decode(path))
}

pub(super) fn file_url_path(file_url: &str) -> Option<PathBuf> {
    let path = file_url
        .split(['?', '#'])
        .next()
        .unwrap_or(file_url)
        .strip_prefix("file://")?;
    Some(PathBuf::from(percent_decode(path)))
}

fn classify_plain_utf8_text(event: &Event) -> Option<ClassifiedEvent> {
    if event.items.len() != 1 {
        return None;
    }

    let data = find_data_in_item(&event.items[0], "public.utf8-plain-text")?;
    Some(ClassifiedEvent {
        content_hash: hash_bytes(&data.data),
        data_type: "text".to_string(),
        display: data.data.clone(),
    })
}

fn image_file_url_type(item: &Item, file_url_data: &Data) -> Option<String> {
    let file_url = String::from_utf8_lossy(&file_url_data.data);
    if file_url.ends_with('/') {
        return None;
    }

    let extension = file_url_extension(&file_url);
    if extension
        .as_deref()
        .is_some_and(is_supported_image_extension)
    {
        return extension;
    }

    if find_data_in_item(item, "public.tiff").is_some_and(|data| !data.data.is_empty()) {
        return Some("tiff".to_string());
    }

    None
}

fn is_video_file_url(file_url_data: &Data) -> bool {
    let file_url = String::from_utf8_lossy(&file_url_data.data);
    file_url_extension(&file_url)
        .as_deref()
        .is_some_and(is_supported_video_extension)
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
        let file_url = find_data_in_item(item, "public.file-url")?;
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
    .unwrap_or_else(|_| label_for_data_type("files").into_bytes())
}

fn file_display_item(item: &Item, display_name: Option<String>) -> Option<FileDisplayItem> {
    let file_url = find_data_in_item(item, "public.file-url")?;
    let file_url = String::from_utf8_lossy(&file_url.data);
    let item_type = if file_url.ends_with('/') {
        "folder"
    } else {
        "file"
    };
    Some(file_display_item_for_url(
        item_type,
        &file_url,
        display_name.or_else(|| file_display_name_in_item(item)),
    ))
}

fn file_display_item_for_url(
    item_type: &str,
    file_url: &str,
    display_name: Option<String>,
) -> FileDisplayItem {
    let name = display_name
        .or_else(|| {
            file_url_display_name(file_url).filter(|name| !is_file_reference_display_name(name))
        })
        .unwrap_or_default();
    FileDisplayItem {
        item_type: item_type.to_string(),
        name,
    }
}

fn file_display_names(event: &Event) -> Vec<String> {
    event
        .items
        .iter()
        .find_map(find_raw_utf8_display_in_item)
        .map(|display| split_file_display_names(&display))
        .unwrap_or_default()
}

fn file_display_name_in_item(item: &Item) -> Option<String> {
    find_raw_utf8_display_in_item(item)
        .and_then(|display| split_file_display_names(&display).into_iter().next())
}

fn split_file_display_names(display: &str) -> Vec<String> {
    display
        .split('\r')
        .filter_map(safe_text_file_display_name)
        .collect()
}

fn safe_text_file_display_name(display: &str) -> Option<String> {
    let display = display
        .trim_matches(|ch: char| ch == '\0' || ch.is_whitespace())
        .to_string();
    if display.is_empty() || is_aggregate_file_label(&display) {
        return None;
    }

    let display_name = path_display_name(&display).unwrap_or(display);
    if is_file_reference_display_name(&display_name) {
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

fn path_display_name(path: &str) -> Option<String> {
    path.rsplit('/')
        .find(|part| !part.is_empty())
        .map(str::to_string)
}

fn display_bytes(display: String) -> Vec<u8> {
    normalize_text(&display).into_bytes()
}

fn classified_from_single_data(
    data_type: &str,
    hash_value: &[u8],
    display: Vec<u8>,
) -> ClassifiedEvent {
    ClassifiedEvent {
        content_hash: hash_bytes(hash_value),
        data_type: data_type.to_string(),
        display,
    }
}

pub(super) fn hash_bytes(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    format!("{:x}", hasher.finalize())
}

fn find_utf8_display(event: &Event) -> Option<String> {
    event.items.iter().find_map(find_utf8_display_in_item)
}

fn find_raw_utf8_display_in_item(item: &Item) -> Option<String> {
    find_data_in_item(item, "public.utf8-plain-text")
        .map(|data| String::from_utf8_lossy(&data.data).into_owned())
}

fn find_utf8_display_in_item(item: &Item) -> Option<String> {
    find_data_in_item(item, "public.utf8-plain-text")
        .map(|data| String::from_utf8_lossy(&data.data).into_owned())
        .map(|text| normalize_text(&text))
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

fn event_contains_attachment(event: &Event) -> bool {
    event
        .items
        .iter()
        .flat_map(|item| &item.data_list)
        .any(|data| {
            let data_type = data.r#type.to_ascii_lowercase();
            matches!(
                data_type.as_str(),
                "public.file-url"
                    | "public.image"
                    | "public.png"
                    | "public.tiff"
                    | "public.jpeg"
                    | "public.jpg"
                    | "public.gif"
                    | "public.heic"
                    | "public.webp"
                    | "public.bmp"
                    | "public.movie"
                    | "public.video"
            ) || (data_type == "public.html" && html_contains_attachment(&data.data))
                || (data_type == "public.rtf" && rtf_contains_attachment(&data.data))
        })
}

fn html_contains_attachment(data: &[u8]) -> bool {
    let html = String::from_utf8_lossy(data).to_ascii_lowercase();
    ["<img", "<picture", "<video", "<object", "<embed"]
        .iter()
        .any(|tag| html.contains(tag))
}

fn rtf_contains_attachment(data: &[u8]) -> bool {
    String::from_utf8_lossy(data)
        .to_ascii_lowercase()
        .contains("\\pict")
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (
                hex_digit_value(bytes[index + 1]),
                hex_digit_value(bytes[index + 2]),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn event(data_list: Vec<Data>) -> Event {
        Event {
            items: vec![Item { data_list }],
        }
    }

    #[test]
    fn classification_is_deterministic_and_ignores_private_metadata_for_identity() {
        let plain = Data {
            r#type: "public.utf8-plain-text".to_string(),
            data: b"hello".to_vec(),
        };
        let first = classify_event(&event(vec![plain.clone()])).unwrap();
        let second = classify_event(&event(vec![
            plain,
            Data {
                r#type: SOURCE_TYPE.to_string(),
                data: b"com.example.source".to_vec(),
            },
        ]))
        .unwrap();
        assert_eq!(first.content_hash, second.content_hash);
        assert_eq!(first.data_type, "text");
    }

    #[test]
    fn compact_projection_preserves_protocol_markers_but_rejects_attachments() {
        let projected = compact_text_event(&event(vec![
            Data {
                r#type: "public.utf8-plain-text".to_string(),
                data: b"hello".to_vec(),
            },
            Data {
                r#type: REMOTE_CLIPBOARD_TYPE.to_string(),
                data: Vec::new(),
            },
        ]))
        .unwrap();
        assert!(find_data(&projected, REMOTE_CLIPBOARD_TYPE).is_some());

        assert!(compact_text_event(&event(vec![
            Data {
                r#type: "public.utf8-plain-text".to_string(),
                data: b"hello".to_vec(),
            },
            Data {
                r#type: "public.png".to_string(),
                data: vec![1, 2, 3],
            },
        ]))
        .is_none());
    }

    #[test]
    fn file_display_parser_rejects_unknown_format() {
        let encoded = serde_json::to_vec(&FileDisplay {
            format: "unknown".to_string(),
            items: Vec::new(),
        })
        .unwrap();
        assert!(parse_file_display(&encoded).is_none());
    }
}
