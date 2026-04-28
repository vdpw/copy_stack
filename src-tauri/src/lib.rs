// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
pub mod event;
mod store;
mod tray;

use crate::store::{AppSettings, Database, StoredEvent};
use copy_event_listener::clipboard::ClipboardListener;
use copy_event_listener::event::Event;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State, WindowEvent};

// State to hold the database
pub struct AppState {
    pub(crate) db: Mutex<Database>,
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
    let event = {
        let db = state.db.lock().unwrap();
        db.get_event_by_id(&id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Event not found: {}", id))?
    };

    set_clipboard_event_on_main_thread(&app_handle, event)?;

    {
        let db = state.db.lock().unwrap();
        db.move_event_to_top(&id).map_err(|e| e.to_string())?;
    }

    tray::sync(&app_handle)?;
    tray::notify_history_changed(&app_handle)?;

    Ok(())
}

fn set_clipboard_event_on_main_thread(app_handle: &AppHandle, event: Event) -> Result<(), String> {
    let (result_tx, result_rx) = channel();

    app_handle
        .run_on_main_thread(move || {
            let result = ClipboardListener::new()
                .set_clipboard_event(event)
                .map_err(|e| format!("Failed to set clipboard: {}", e));
            let _ = result_tx.send(result);
        })
        .map_err(|e| format!("Failed to schedule clipboard update: {}", e))?;

    result_rx
        .recv()
        .map_err(|e| format!("Failed to receive clipboard update result: {}", e))?
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
            let app_handle = app.handle();
            let db = Database::new(&app_handle).expect("Failed to initialize database");

            // Update the app state with the database
            app.manage(AppState { db: Mutex::new(db) });

            // Clean up old events on startup to respect max_items limit
            if let Ok(db) = app.state::<AppState>().db.lock() {
                let _ = db.cleanup_old_events();
            }

            tray::setup(&app_handle).expect("Failed to initialize tray");

            // Handle incoming copy events
            let app_handle_clone = app_handle.clone();
            std::thread::spawn(move || {
                for event in rx {
                    let insert_result = {
                        let state = app_handle_clone.state::<AppState>();
                        let db = state.db.lock().unwrap();
                        db.insert_event(&event).map_err(|error| error.to_string())
                    };

                    if let Err(error) = insert_result {
                        eprintln!("failed to store clipboard item: {}", error);
                        continue;
                    }

                    if let Err(error) = tray::sync(&app_handle_clone) {
                        eprintln!("failed to refresh tray menu: {}", error);
                    }
                    if let Err(error) = tray::notify_history_changed(&app_handle_clone) {
                        eprintln!("failed to notify frontend: {}", error);
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
            set_show_in_menu_bar
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
