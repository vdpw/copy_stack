use crate::i18n::{native_strings, Language};
use crate::pasteboard_protocol::prepare_event_for_restore;
use crate::store::{Database, FileDisplayItem, TrayEvent};
use crate::{
    clear_restore_suppression_if_matches, queue_restore_suppression,
    report_restore_post_processing_failure, report_tray_operation_failure,
    restore_event_to_clipboard, schedule_history_mirror_for_tray, AppState,
};
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{image::Image, AppHandle, Emitter, Manager, Runtime};

const TRAY_ID: &str = "main";
const EVENT_ITEM_PREFIX: &str = "event::";
const OPEN_HISTORY_ID: &str = "action::open-history";
const OPEN_SETTINGS_ID: &str = "action::open-settings";
const CLEAR_HISTORY_ID: &str = "action::clear-history";
const QUIT_ID: &str = "action::quit";
const HEADER_ID: &str = "label::recent-items";
const EMPTY_STATE_ID: &str = "label::empty";
const MAX_MENU_LABEL_WIDTH: usize = 40;
const TRUNCATION_SUFFIX: &str = "...";
const ERROR_APP_STATE_UNAVAILABLE: &str = "app_state_unavailable";
const ERROR_CLIPBOARD_ITEM_UNAVAILABLE: &str = "clipboard_item_unavailable";
const ERROR_CLIPBOARD_RESTORE_FAILED: &str = "clipboard_restore_failed";
const ERROR_HISTORY_OPERATION_FAILED: &str = "history_operation_failed";
const ERROR_MAIN_WINDOW_UNAVAILABLE: &str = "main_window_unavailable";
const ERROR_MENU_BUILD_FAILED: &str = "menu_build_failed";
const ERROR_TRAY_OPERATION_FAILED: &str = "tray_operation_failed";
const ERROR_WINDOW_OPERATION_FAILED: &str = "window_operation_failed";

pub const HISTORY_UPDATED_EVENT: &str = "clipboard-history-updated";
pub const LANGUAGE_CHANGED_EVENT: &str = "app-language-changed";
pub const NAVIGATE_EVENT: &str = "app:navigate";
pub const HISTORY_PAGE: &str = "history";
pub const SETTINGS_PAGE: &str = "settings";

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let menu = build_menu(app)?;
    let icon = Image::from_bytes(include_bytes!("../icons/tray-template.png"))
        .map_err(|_| ERROR_TRAY_OPERATION_FAILED.to_string())?;

    TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("Copy Stack")
        .show_menu_on_left_click(true)
        .icon(icon)
        .icon_as_template(true)
        .on_menu_event(|app, event| {
            if let Err(_error) = handle_menu_event(app, event.id().as_ref()) {
                report_tray_operation_failure(app);
                debug_error!("tray menu action failed: {}", _error);
            }
        })
        .build(app)
        .map_err(|_| ERROR_TRAY_OPERATION_FAILED.to_string())?;
    sync(app)
}

pub fn sync<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| ERROR_TRAY_OPERATION_FAILED.to_string())?;
    let menu = build_menu(app)?;
    tray.set_menu(Some(menu))
        .map_err(|_| ERROR_TRAY_OPERATION_FAILED.to_string())?;

    let show_in_menu_bar = {
        let state = app.state::<AppState>();
        let db = state
            .db
            .lock()
            .map_err(|_| ERROR_APP_STATE_UNAVAILABLE.to_string())?;
        db.get_show_in_menu_bar()
            .map_err(|_| ERROR_HISTORY_OPERATION_FAILED.to_string())?
    };
    tray.set_visible(show_in_menu_bar)
        .map_err(|_| ERROR_TRAY_OPERATION_FAILED.to_string())
}

pub fn show_page<R: Runtime>(app: &AppHandle<R>, page: &str) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| ERROR_MAIN_WINDOW_UNAVAILABLE.to_string())?;

    window
        .show()
        .map_err(|_| ERROR_WINDOW_OPERATION_FAILED.to_string())?;
    window
        .unminimize()
        .map_err(|_| ERROR_WINDOW_OPERATION_FAILED.to_string())?;
    window
        .set_focus()
        .map_err(|_| ERROR_WINDOW_OPERATION_FAILED.to_string())?;

    app.emit(NAVIGATE_EVENT, page.to_string())
        .map_err(|_| ERROR_WINDOW_OPERATION_FAILED.to_string())
}

pub fn notify_history_changed<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    app.emit(HISTORY_UPDATED_EVENT, ())
        .map_err(|_| ERROR_WINDOW_OPERATION_FAILED.to_string())
}

pub(crate) fn notify_language_changed<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    app.emit(LANGUAGE_CHANGED_EVENT, ())
        .map_err(|_| ERROR_WINDOW_OPERATION_FAILED.to_string())
}

fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, menu_id: &str) -> Result<(), String> {
    match menu_id {
        OPEN_HISTORY_ID => show_page(app, HISTORY_PAGE),
        OPEN_SETTINGS_ID => show_page(app, SETTINGS_PAGE),
        CLEAR_HISTORY_ID => {
            let state = app.state::<AppState>();
            {
                let db = state
                    .db
                    .lock()
                    .map_err(|_| ERROR_APP_STATE_UNAVAILABLE.to_string())?;
                db.clear_all_events()
                    .map_err(|_| ERROR_HISTORY_OPERATION_FAILED.to_string())?;
            }
            schedule_history_mirror_for_tray(&state)
                .map_err(|_| ERROR_HISTORY_OPERATION_FAILED.to_string())?;
            notify_history_changed(app)?;
            sync(app)
        }
        QUIT_ID => {
            app.exit(0);
            Ok(())
        }
        _ if menu_id.starts_with(EVENT_ITEM_PREFIX) => {
            let content_hash = &menu_id[EVENT_ITEM_PREFIX.len()..];
            restore_event(app, content_hash)
        }
        _ => Ok(()),
    }
}

fn restore_event<R: Runtime>(app: &AppHandle<R>, content_hash: &str) -> Result<(), String> {
    let (seed, move_restored_item_to_top) = {
        let state = app.state::<AppState>();
        let db = state
            .db
            .lock()
            .map_err(|_| ERROR_APP_STATE_UNAVAILABLE.to_string())?;
        let seed = db
            .get_restore_seed(content_hash)
            .map_err(|_| ERROR_HISTORY_OPERATION_FAILED.to_string())?
            .ok_or_else(|| ERROR_CLIPBOARD_ITEM_UNAVAILABLE.to_string())?;
        let move_restored_item_to_top = db
            .get_move_restored_item_to_top()
            .map_err(|_| ERROR_HISTORY_OPERATION_FAILED.to_string())?;
        (seed, move_restored_item_to_top)
    };
    let restore_content_hash = seed.content_hash.clone();
    let source_bundle_id = seed.source_bundle_id.clone();
    let is_remote_clipboard = seed.is_remote_clipboard;
    let event = seed
        .into_event()
        .map_err(|_| ERROR_CLIPBOARD_ITEM_UNAVAILABLE.to_string())?
        .ok_or_else(|| ERROR_CLIPBOARD_ITEM_UNAVAILABLE.to_string())?;
    let event = prepare_event_for_restore(event, source_bundle_id.as_deref(), is_remote_clipboard)
        .map_err(|_| ERROR_CLIPBOARD_ITEM_UNAVAILABLE.to_string())?;

    if !move_restored_item_to_top {
        let state = app.state::<AppState>();
        queue_restore_suppression(&state, restore_content_hash.clone());
    }

    if restore_event_to_clipboard(event).is_err() {
        let state = app.state::<AppState>();
        clear_restore_suppression_if_matches(&state, &restore_content_hash);
        return Err(ERROR_CLIPBOARD_RESTORE_FAILED.to_string());
    }

    if move_restored_item_to_top {
        let state = app.state::<AppState>();
        let post_processing_result = (|| -> Result<(), String> {
            {
                let db = state
                    .db
                    .lock()
                    .map_err(|_| ERROR_APP_STATE_UNAVAILABLE.to_string())?;
                db.move_event_to_top(content_hash)
                    .map_err(|_| ERROR_HISTORY_OPERATION_FAILED.to_string())?;
            }
            schedule_history_mirror_for_tray(&state)
                .map_err(|_| ERROR_HISTORY_OPERATION_FAILED.to_string())?;
            notify_history_changed(app)?;
            sync(app)
        })();
        if post_processing_result.is_err() {
            report_restore_post_processing_failure(app, &state);
        }
    }

    Ok(())
}

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> Result<Menu<R>, String> {
    let (events, language) = {
        let state = app.state::<AppState>();
        let db = state
            .db
            .lock()
            .map_err(|_| ERROR_APP_STATE_UNAVAILABLE.to_string())?;
        (
            db.get_tray_events()
                .map_err(|_| ERROR_HISTORY_OPERATION_FAILED.to_string())?,
            db.get_language()
                .map_err(|_| ERROR_HISTORY_OPERATION_FAILED.to_string())?
                .resolve(),
        )
    };
    let strings = native_strings(language);

    let recent_items = MenuItemBuilder::with_id(HEADER_ID, strings.recent_clipboard_items)
        .enabled(false)
        .build(app)
        .map_err(|_| ERROR_MENU_BUILD_FAILED.to_string())?;
    let empty_state = MenuItemBuilder::with_id(EMPTY_STATE_ID, strings.no_clipboard_items)
        .enabled(false)
        .build(app)
        .map_err(|_| ERROR_MENU_BUILD_FAILED.to_string())?;
    let open_history = MenuItemBuilder::with_id(OPEN_HISTORY_ID, strings.open_history)
        .build(app)
        .map_err(|_| ERROR_MENU_BUILD_FAILED.to_string())?;
    let open_settings = MenuItemBuilder::with_id(OPEN_SETTINGS_ID, strings.open_settings)
        .build(app)
        .map_err(|_| ERROR_MENU_BUILD_FAILED.to_string())?;
    let clear_history = MenuItemBuilder::with_id(CLEAR_HISTORY_ID, strings.clear_history)
        .enabled(!events.is_empty())
        .build(app)
        .map_err(|_| ERROR_MENU_BUILD_FAILED.to_string())?;
    let quit = MenuItemBuilder::with_id(QUIT_ID, strings.quit_copy_stack)
        .build(app)
        .map_err(|_| ERROR_MENU_BUILD_FAILED.to_string())?;

    let mut builder = MenuBuilder::new(app).item(&recent_items).separator();

    if events.is_empty() {
        builder = builder.item(&empty_state);
    } else {
        for event in &events {
            let menu_label = event_menu_label(event, language);
            let event_item_id = format!("{}{}", EVENT_ITEM_PREFIX, event.content_hash.as_str());
            let item = MenuItemBuilder::with_id(event_item_id, menu_label)
                .build(app)
                .map_err(|_| ERROR_MENU_BUILD_FAILED.to_string())?;
            builder = builder.item(&item);
        }
    }

    builder
        .separator()
        .item(&open_history)
        .item(&open_settings)
        .item(&clear_history)
        .separator()
        .item(&quit)
        .build()
        .map_err(|_| ERROR_MENU_BUILD_FAILED.to_string())
}

fn event_menu_label(event: &TrayEvent, language: Language) -> String {
    truncate_label(event_menu_full_label(event, language))
}

fn event_menu_full_label(event: &TrayEvent, language: Language) -> String {
    if let Some(file_display) = Database::parse_file_display(&event.display) {
        return file_display
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| file_menu_item_label(item, index, language))
            .collect::<Vec<_>>()
            .join("  ");
    }

    let label = display_label(event, language);
    match event.data_type.as_str() {
        "file" | "files" => format!("📄 {}", label),
        "folder" | "folders" => format!("📁 {}", label),
        _ => label,
    }
}

fn file_menu_item_label(item: &FileDisplayItem, index: usize, language: Language) -> String {
    let strings = native_strings(language);
    let icon = match item.item_type.as_str() {
        "folder" => "📁",
        _ => "📄",
    };
    let name = if item.name.is_empty() {
        let fallback = if item.item_type == "folder" {
            strings.folder
        } else {
            strings.file
        };
        format!("{} {}", fallback, index + 1)
    } else {
        item.name.clone()
    };
    format!("{} {}", icon, name)
}

fn display_label(event: &TrayEvent, language: Language) -> String {
    let strings = native_strings(language);
    let label = String::from_utf8_lossy(&event.display);
    let normalized = label.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() || label.contains('\u{fffd}') {
        match event.data_type.as_str() {
            "file" => strings.file.to_string(),
            "folder" => strings.folder.to_string(),
            "files" => strings.files.to_string(),
            "folders" => strings.folders.to_string(),
            "files and folders" => strings.files_and_folders.to_string(),
            "video" => strings.video.to_string(),
            _ => event.data_type.to_uppercase(),
        }
    } else if event.data_type == "video" && normalized == "Video" {
        strings.video.to_string()
    } else if event.data_type == "files" && normalized == "Files" {
        strings.files.to_string()
    } else if event.data_type == "folders" && normalized == "Folders" {
        strings.folders.to_string()
    } else if event.data_type == "files and folders" && normalized == "Files and folders" {
        strings.files_and_folders.to_string()
    } else {
        normalized
    }
}

fn truncate_label(value: String) -> String {
    if display_width(&value) <= MAX_MENU_LABEL_WIDTH {
        return value;
    }

    let suffix_width = display_width(TRUNCATION_SUFFIX);
    let available_width = MAX_MENU_LABEL_WIDTH.saturating_sub(suffix_width);
    let mut truncated = String::new();
    let mut current_width = 0;

    for character in value.chars() {
        let character_width = character_display_width(character);
        if current_width + character_width > available_width {
            break;
        }

        truncated.push(character);
        current_width += character_width;
    }

    format!("{}{}", truncated, TRUNCATION_SUFFIX)
}

fn display_width(value: &str) -> usize {
    value.chars().map(character_display_width).sum()
}

fn character_display_width(character: char) -> usize {
    if matches!(
        character,
        '\u{1100}'..='\u{115F}'
            | '\u{2329}'..='\u{232A}'
            | '\u{2E80}'..='\u{A4CF}'
            | '\u{AC00}'..='\u{D7A3}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{FE10}'..='\u{FE19}'
            | '\u{FE30}'..='\u{FE6F}'
            | '\u{FF00}'..='\u{FF60}'
            | '\u{FFE0}'..='\u{FFE6}'
            | '\u{1F300}'..='\u{1FAFF}'
    ) {
        return 2;
    }

    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_label_caps_ascii_width() {
        let label = truncate_label("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNO".to_string());

        assert_eq!(display_width(&label), 40);
        assert!(label.ends_with(TRUNCATION_SUFFIX));
    }

    #[test]
    fn truncate_label_caps_cjk_width() {
        let label = truncate_label("复制历史文件夹名称非常非常非常长而且还要继续显示".to_string());

        assert!(display_width(&label) <= 40);
        assert!(label.ends_with(TRUNCATION_SUFFIX));
    }

    #[test]
    fn generated_file_names_are_localized_at_presentation_time() {
        let file = FileDisplayItem {
            item_type: "file".to_string(),
            name: String::new(),
        };
        let folder = FileDisplayItem {
            item_type: "folder".to_string(),
            name: String::new(),
        };

        assert_eq!(
            file_menu_item_label(&file, 0, Language::SimplifiedChinese),
            "📄 文件 1"
        );
        assert_eq!(
            file_menu_item_label(&folder, 1, Language::TraditionalChinese),
            "📁 資料夾 2"
        );
    }

    #[test]
    fn aggregate_file_fallbacks_are_localized() {
        let event = TrayEvent {
            content_hash: "hash".to_string(),
            data_type: "files and folders".to_string(),
            display: Vec::new(),
        };

        assert_eq!(
            display_label(&event, Language::TraditionalChinese),
            "檔案和資料夾"
        );
    }

    #[test]
    fn first_and_last_tray_labels_preserve_boundary_items() {
        let events = (1..=25)
            .map(|index| TrayEvent {
                content_hash: format!("{index:064x}"),
                data_type: "text".to_string(),
                display: format!("clipboard item {index}").into_bytes(),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            event_menu_label(&events[0], Language::English),
            "clipboard item 1"
        );
        assert_eq!(
            event_menu_label(&events[24], Language::English),
            "clipboard item 25"
        );
    }
}
