// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
pub mod event;
mod store;

use crate::store::{Database, StoredEvent};
use copy_event_listener::clipboard::ClipboardListener;
use copy_event_listener::event::Event;
use std::sync::mpsc::Receiver;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

// State to hold the database
pub struct AppState {
    db: Mutex<Database>,
}

fn emit_events_updated(app_handle: &AppHandle) {
    let _ = app_handle.emit("copy-events-updated", ());
}

#[tauri::command]
async fn get_copy_events(state: State<'_, AppState>) -> Result<Vec<StoredEvent>, String> {
    let db = state.db.lock().unwrap();
    db.get_all_events().map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_copy_event(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    db.delete_event(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn clear_all_events(state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    db.clear_all_events().map_err(|e| e.to_string())
}

#[tauri::command]
async fn copy_to_clipboard(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let event = {
        let db = state.db.lock().unwrap();
        db.get_event_by_id(&id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Event not found: {}", id))?
    };

    let listener = ClipboardListener::new();
    listener
        .set_clipboard_event(event)
        .map_err(|e| format!("Failed to set clipboard: {}", e))?;

    {
        let db = state.db.lock().unwrap();
        db.move_event_to_top(&id).map_err(|e| e.to_string())?;
    }

    emit_events_updated(&app_handle);

    Ok(())
}

#[tauri::command]
async fn get_event_by_id(state: State<'_, AppState>, id: String) -> Result<Option<Event>, String> {
    let db = state.db.lock().unwrap();
    db.get_event_by_id(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_max_items(state: State<'_, AppState>) -> Result<u32, String> {
    let db = state.db.lock().unwrap();
    db.get_max_items().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_max_items(state: State<'_, AppState>, max_items: u32) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    db.set_max_items(max_items).map_err(|e| e.to_string())?;
    // Clean up old events after setting new limit
    db.cleanup_old_events().map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(rx: Receiver<Event>) {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle();
            let db = Database::new(&app_handle).expect("Failed to initialize database");

            // Update the app state with the database
            app.manage(AppState { db: Mutex::new(db) });

            // Clean up old events on startup to respect max_items limit
            if let Ok(db) = app.state::<AppState>().db.lock() {
                let _ = db.cleanup_old_events();
            }

            // Handle incoming copy events
            let app_handle_clone = app_handle.clone();
            std::thread::spawn(move || {
                for event in rx {
                    let mut updated = false;

                    if let Ok(db) = app_handle_clone.state::<AppState>().db.lock() {
                        updated = db.insert_event(&event).is_ok();
                    }

                    if updated {
                        emit_events_updated(&app_handle_clone);
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
            get_max_items,
            set_max_items
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
