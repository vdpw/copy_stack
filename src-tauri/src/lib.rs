// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
pub mod event;
mod store;

use copy_event_listener::event::Event;
use copy_event_listener::clipboard::ClipboardListener;
use crate::store::{Database, StoredEvent};
use std::sync::mpsc::Receiver;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};

// State to hold the database
pub struct AppState {
    db: Mutex<Database>,
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
async fn copy_to_clipboard(event_data: String) -> Result<(), String> {
    // Deserialize the event data
    let event: Event = serde_json::from_str(&event_data)
        .map_err(|e| format!("Failed to deserialize event: {}", e))?;
    
    // Create a new clipboard listener and set the event
    let listener = ClipboardListener::new();
    listener.set_clipboard_event(event)
        .map_err(|e| format!("Failed to set clipboard: {}", e))
}

#[tauri::command]
async fn get_event_by_id(state: State<'_, AppState>, id: String) -> Result<Option<Event>, String> {
    let db = state.db.lock().unwrap();
    db.get_event_by_id(&id).map_err(|e| e.to_string())
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

            // Handle incoming copy events
            let app_handle_clone = app_handle.clone();
            std::thread::spawn(move || {
                for event in rx {
                    // Store the event in the database
                    if let Ok(db) = app_handle_clone.state::<AppState>().db.lock() {
                        let _ = db.insert_event(&event);
                    }

                    // Emit the event to the frontend
                    let _ = app_handle_clone.emit_to("main", "new-copy-event", event);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_copy_events,
            delete_copy_event,
            clear_all_events,
            copy_to_clipboard,
            get_event_by_id
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
