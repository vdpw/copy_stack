// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            println!($($arg)*);
        }
    };
}

macro_rules! debug_error {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            eprintln!($($arg)*);
        }
    };
}

pub mod event;
mod store;
mod tray;

use crate::store::{AppSettings, Database, StoredEvent};
use copy_event_listener::clipboard::ClipboardListener;
use copy_event_listener::event::Event;
use std::sync::mpsc::Receiver;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State, WindowEvent};

const RESTORE_SUPPRESSION_TTL: Duration = Duration::from_secs(5);

// State to hold the database
pub struct AppState {
    pub(crate) db: Mutex<Database>,
    pub(crate) pending_restore_suppression: Mutex<Option<PendingRestoreSuppression>>,
}

pub(crate) struct PendingRestoreSuppression {
    content_hash: String,
    created_at: Instant,
}

#[tauri::command]
async fn get_copy_events(state: State<'_, AppState>) -> Result<Vec<StoredEvent>, String> {
    let db = state.db.lock().unwrap();
    db.get_all_events().map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_copy_event(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    {
        let db = state.db.lock().unwrap();
        db.delete_event(&id).map_err(|e| e.to_string())?;
    }
    tray::sync(&app)
}

#[tauri::command]
async fn clear_all_events(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let db = state.db.lock().unwrap();
        db.clear_all_events().map_err(|e| e.to_string())?;
    }
    tray::sync(&app)
}

#[tauri::command]
async fn copy_to_clipboard(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    debug_log!("[copy_stack] copy_to_clipboard requested: id={}", id);

    let (event, content_hash, move_restored_item_to_top) = {
        let db = state.db.lock().unwrap();
        let event = db
            .get_event_by_id(&id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Event not found: {}", id))?;
        let content_hash = db.event_content_hash(&event).map_err(|e| e.to_string())?;
        let move_restored_item_to_top = db
            .get_move_restored_item_to_top()
            .map_err(|e| e.to_string())?;
        (event, content_hash, move_restored_item_to_top)
    };
    debug_log!("[copy_stack] clipboard event loaded: id={}", id);

    if !move_restored_item_to_top {
        queue_restore_suppression(&state, content_hash.clone());
        debug_log!("[copy_stack] restore will preserve list order: id={}", id);
    }

    if let Err(error) = restore_event_to_clipboard(event) {
        clear_restore_suppression_if_matches(&state, &content_hash);
        return Err(error);
    }
    debug_log!(
        "[copy_stack] clipboard event written to pasteboard: id={}",
        id
    );

    if move_restored_item_to_top {
        let db = state.db.lock().unwrap();
        db.move_event_to_top(&id).map_err(|e| e.to_string())?;
        debug_log!("[copy_stack] clipboard event moved to top: id={}", id);
    } else {
        debug_log!("[copy_stack] clipboard event order unchanged: id={}", id);
    }

    tray::sync(&app_handle)?;
    tray::notify_history_changed(&app_handle)?;
    debug_log!("[copy_stack] history/tray refresh notified: id={}", id);

    Ok(())
}

pub(crate) fn restore_event_to_clipboard(event: Event) -> Result<(), String> {
    debug_log!("[copy_stack] writing clipboard event to pasteboard");
    ClipboardListener::new()
        .set_clipboard_event(event)
        .map_err(|e| format!("Failed to set clipboard: {}", e))
}

pub(crate) fn queue_restore_suppression(state: &AppState, content_hash: String) {
    let mut pending = state.pending_restore_suppression.lock().unwrap();
    *pending = Some(PendingRestoreSuppression {
        content_hash,
        created_at: Instant::now(),
    });
}

pub(crate) fn clear_restore_suppression_if_matches(state: &AppState, content_hash: &str) {
    let mut pending = state.pending_restore_suppression.lock().unwrap();
    if pending
        .as_ref()
        .is_some_and(|suppression| suppression.content_hash == content_hash)
    {
        *pending = None;
    }
}

fn should_skip_pending_restore_event(state: &AppState, content_hash: &str) -> bool {
    let mut pending = state.pending_restore_suppression.lock().unwrap();
    let Some(suppression) = pending.as_ref() else {
        return false;
    };

    if suppression.created_at.elapsed() > RESTORE_SUPPRESSION_TTL {
        *pending = None;
        return false;
    }

    if suppression.content_hash != content_hash {
        return false;
    }

    *pending = None;
    true
}

#[tauri::command]
async fn get_event_by_id(state: State<'_, AppState>, id: String) -> Result<Option<Event>, String> {
    let db = state.db.lock().unwrap();
    db.get_event_by_id(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let db = state.db.lock().unwrap();
    db.get_settings().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_max_items(
    app: AppHandle,
    state: State<'_, AppState>,
    max_items: u32,
) -> Result<(), String> {
    {
        let db = state.db.lock().unwrap();
        db.set_max_items(max_items).map_err(|e| e.to_string())?;
        db.cleanup_old_events().map_err(|e| e.to_string())?;
    }
    tray::sync(&app)
}

#[tauri::command]
async fn set_show_in_menu_bar(
    app: AppHandle,
    state: State<'_, AppState>,
    show_in_menu_bar: bool,
) -> Result<(), String> {
    {
        let db = state.db.lock().unwrap();
        db.set_show_in_menu_bar(show_in_menu_bar)
            .map_err(|e| e.to_string())?;
    }
    tray::sync(&app)
}

#[tauri::command]
async fn set_move_restored_item_to_top(
    state: State<'_, AppState>,
    move_restored_item_to_top: bool,
) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    db.set_move_restored_item_to_top(move_restored_item_to_top)
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(rx: Receiver<Event>) {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            debug_log!("[copy_stack] Tauri setup started");
            let app_handle = app.handle();
            let db = Database::new(&app_handle).expect("Failed to initialize database");
            debug_log!("[copy_stack] database initialized");

            // Update the app state with the database
            app.manage(AppState {
                db: Mutex::new(db),
                pending_restore_suppression: Mutex::new(None),
            });

            // Clean up old events on startup to respect max_items limit
            if let Ok(db) = app.state::<AppState>().db.lock() {
                let _ = db.cleanup_old_events();
            }

            tray::setup(&app_handle).expect("Failed to initialize tray");
            debug_log!("[copy_stack] tray initialized");

            // Handle incoming copy events
            let app_handle_clone = app_handle.clone();
            std::thread::spawn(move || {
                for event in rx {
                    debug_log!("[copy_stack] clipboard listener event received");
                    let event_hash = {
                        let state = app_handle_clone.state::<AppState>();
                        let db = state.db.lock().unwrap();
                        db.event_content_hash(&event)
                            .map_err(|error| error.to_string())
                    };

                    if let Ok(event_hash) = event_hash {
                        let state = app_handle_clone.state::<AppState>();
                        if should_skip_pending_restore_event(&state, &event_hash) {
                            debug_log!(
                                "[copy_stack] skipped restored clipboard event to preserve order"
                            );
                            continue;
                        }
                    }

                    debug_log!("[copy_stack] storing clipboard listener event");
                    let insert_result = {
                        let state = app_handle_clone.state::<AppState>();
                        let db = state.db.lock().unwrap();
                        db.insert_event(&event).map_err(|error| error.to_string())
                    };

                    if let Err(_error) = insert_result {
                        debug_error!("failed to store clipboard item: {}", _error);
                        continue;
                    }

                    if let Err(_error) = tray::sync(&app_handle_clone) {
                        debug_error!("failed to refresh tray menu: {}", _error);
                    }
                    if let Err(_error) = tray::notify_history_changed(&app_handle_clone) {
                        debug_error!("failed to notify frontend: {}", _error);
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_copy_events,
            delete_copy_event,
            clear_all_events,
            copy_to_clipboard,
            get_event_by_id,
            get_app_settings,
            set_max_items,
            set_show_in_menu_bar,
            set_move_restored_item_to_top
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
