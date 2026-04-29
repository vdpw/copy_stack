use crate::event::{ClipboardData, ClipboardEvent};
use crate::store::StoredEvent;
use crate::{
    clear_restore_suppression_if_matches, queue_restore_suppression, restore_event_to_clipboard,
    AppState,
};
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Runtime};

const TRAY_ID: &str = "main";
const EVENT_ITEM_PREFIX: &str = "event::";
const OPEN_HISTORY_ID: &str = "action::open-history";
const OPEN_SETTINGS_ID: &str = "action::open-settings";
const CLEAR_HISTORY_ID: &str = "action::clear-history";
const QUIT_ID: &str = "action::quit";
const HEADER_ID: &str = "label::recent-items";
const EMPTY_STATE_ID: &str = "label::empty";
const MAX_MENU_LABEL_LENGTH: usize = 72;

pub const HISTORY_UPDATED_EVENT: &str = "clipboard-history-updated";
pub const NAVIGATE_EVENT: &str = "app:navigate";
pub const HISTORY_PAGE: &str = "history";
pub const SETTINGS_PAGE: &str = "settings";

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let menu = build_menu(app)?;

    let mut tray_builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("Copy Stack")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            if let Err(_error) = handle_menu_event(app, event.id().as_ref()) {
                debug_error!("tray menu action failed: {}", _error);
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray_builder = tray_builder.icon(icon).icon_as_template(true);
    } else {
        tray_builder = tray_builder.title("Copy Stack");
    }

    tray_builder.build(app).map_err(|error| error.to_string())?;
    sync(app)
}

pub fn sync<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "Tray icon not found".to_string())?;
    let menu = build_menu(app)?;
    tray.set_menu(Some(menu))
        .map_err(|error| error.to_string())?;

    let show_in_menu_bar = {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        db.get_show_in_menu_bar()
            .map_err(|error| error.to_string())?
    };
    tray.set_visible(show_in_menu_bar)
        .map_err(|error| error.to_string())
}

pub fn show_page<R: Runtime>(app: &AppHandle<R>, page: &str) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;

    window.show().map_err(|error| error.to_string())?;
    let _ = window.unminimize();
    let _ = window.set_focus();

    app.emit(NAVIGATE_EVENT, page.to_string())
        .map_err(|error| error.to_string())
}

pub fn notify_history_changed<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    app.emit(HISTORY_UPDATED_EVENT, ())
        .map_err(|error| error.to_string())
}

fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, menu_id: &str) -> Result<(), String> {
    match menu_id {
        OPEN_HISTORY_ID => show_page(app, HISTORY_PAGE),
        OPEN_SETTINGS_ID => show_page(app, SETTINGS_PAGE),
        CLEAR_HISTORY_ID => {
            {
                let state = app.state::<AppState>();
                let db = state.db.lock().unwrap();
                db.clear_all_events().map_err(|error| error.to_string())?;
            }
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
    let (event, restore_content_hash, move_restored_item_to_top) = {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        let event = db
            .get_event_by_content_hash(content_hash)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Clipboard item not found: {}", content_hash))?;
        let restore_content_hash = db
            .event_content_hash(&event)
            .map_err(|error| error.to_string())?;
        let move_restored_item_to_top = db
            .get_move_restored_item_to_top()
            .map_err(|error| error.to_string())?;
        (event, restore_content_hash, move_restored_item_to_top)
    };

    if !move_restored_item_to_top {
        let state = app.state::<AppState>();
        queue_restore_suppression(&state, restore_content_hash.clone());
    }

    if let Err(error) = restore_event_to_clipboard(event) {
        let state = app.state::<AppState>();
        clear_restore_suppression_if_matches(&state, &restore_content_hash);
        return Err(error);
    }

    if move_restored_item_to_top {
        {
            let state = app.state::<AppState>();
            let db = state.db.lock().unwrap();
            db.move_event_to_top(&restore_content_hash)
                .map_err(|error| error.to_string())?;
        }
        notify_history_changed(app)?;
        sync(app)?;
    }

    Ok(())
}

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> Result<Menu<R>, String> {
    let events = {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        db.get_all_events().map_err(|error| error.to_string())?
    };

    let recent_items = MenuItemBuilder::with_id(HEADER_ID, "Recent clipboard items")
        .enabled(false)
        .build(app)
        .map_err(|error| error.to_string())?;
    let empty_state = MenuItemBuilder::with_id(EMPTY_STATE_ID, "No clipboard items yet")
        .enabled(false)
        .build(app)
        .map_err(|error| error.to_string())?;
    let open_history = MenuItemBuilder::with_id(OPEN_HISTORY_ID, "Open history")
        .build(app)
        .map_err(|error| error.to_string())?;
    let open_settings = MenuItemBuilder::with_id(OPEN_SETTINGS_ID, "Open settings")
        .build(app)
        .map_err(|error| error.to_string())?;
    let clear_history = MenuItemBuilder::with_id(CLEAR_HISTORY_ID, "Clear history")
        .enabled(!events.is_empty())
        .build(app)
        .map_err(|error| error.to_string())?;
    let quit = MenuItemBuilder::with_id(QUIT_ID, "Quit Copy Stack")
        .build(app)
        .map_err(|error| error.to_string())?;

    let mut builder = MenuBuilder::new(app).item(&recent_items).separator();

    if events.is_empty() {
        builder = builder.item(&empty_state);
    } else {
        for event in &events {
            let item = MenuItemBuilder::with_id(
                format!("{}{}", EVENT_ITEM_PREFIX, event.content_hash.as_str()),
                event_menu_label(event),
            )
            .build(app)
            .map_err(|error| error.to_string())?;
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
        .map_err(|error| error.to_string())
}

fn event_menu_label(event: &StoredEvent) -> String {
    truncate_label(extract_preview(&event.event_data))
}

fn extract_preview(event: &ClipboardEvent) -> String {
    for item in &event.items {
        for data in &item.data_list {
            if is_plain_text(&data.r#type) {
                let text = decode_text(data);
                let normalized = normalize_whitespace(&text);
                if !normalized.is_empty() {
                    return normalized;
                }
            }
        }
    }

    event
        .items
        .iter()
        .flat_map(|item| item.data_list.iter())
        .map(|data| format!("[{}]", format_data_type(&data.r#type)))
        .next()
        .unwrap_or_else(|| "Empty clipboard".to_string())
}

fn is_plain_text(data_type: &str) -> bool {
    matches!(
        data_type,
        "public.utf8-plain-text" | "public.utf16-plain-text" | "NSStringPboardType"
    )
}

fn decode_text(data: &ClipboardData) -> String {
    match data.r#type.as_str() {
        "public.utf16-plain-text" => decode_utf16(&data.data)
            .unwrap_or_else(|| String::from_utf8_lossy(&data.data).into_owned()),
        _ => String::from_utf8_lossy(&data.data).into_owned(),
    }
}

fn decode_utf16(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 || bytes.len() % 2 != 0 {
        return None;
    }

    let mut units = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();

    if matches!(units.first(), Some(0xfeff)) {
        units.remove(0);
    }

    String::from_utf16(&units).ok()
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_label(value: String) -> String {
    let mut chars = value.chars();
    let truncated = chars
        .by_ref()
        .take(MAX_MENU_LABEL_LENGTH)
        .collect::<String>();
    if chars.next().is_some() {
        format!("{}…", truncated)
    } else {
        truncated
    }
}

fn format_data_type(data_type: &str) -> String {
    data_type
        .trim_start_matches("public.")
        .replace('.', " ")
        .replace('-', " ")
}
