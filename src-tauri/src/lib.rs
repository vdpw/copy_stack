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
mod i18n;
mod startup;
mod store;
mod tray;

pub use startup::StartupOptions;

use crate::event::ClipboardEvent;
use crate::i18n::{native_strings, Language, LanguagePreference};
use crate::store::{AppSettings, Database, HistoryJsonlConfig, StoredEvent};
use copy_event_listener::clipboard::ClipboardListener;
use copy_event_listener::event::Event;
use std::sync::mpsc::Receiver;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{
    AboutMetadata, Menu, MenuItemBuilder, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID,
    WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Manager, Runtime, State, WindowEvent};

const RESTORE_SUPPRESSION_TTL: Duration = Duration::from_secs(5);
const OPEN_APP_SETTINGS_ID: &str = "app-menu::open-settings";

// State to hold the database
pub struct AppState {
    pub(crate) db: Mutex<Database>,
    pub(crate) pending_restore_suppression: Mutex<Option<PendingRestoreSuppression>>,
    pub(crate) history_jsonl: Option<HistoryJsonlConfig>,
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
    content_hash: String,
) -> Result<(), String> {
    {
        let db = state.db.lock().unwrap();
        db.delete_event(&content_hash).map_err(|e| e.to_string())?;
        write_history_jsonl_if_enabled(&db, state.history_jsonl.as_ref(), "delete_copy_event");
    }
    tray::sync(&app)
}

#[tauri::command]
async fn clear_all_events(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let db = state.db.lock().unwrap();
        db.clear_all_events().map_err(|e| e.to_string())?;
        write_history_jsonl_if_enabled(&db, state.history_jsonl.as_ref(), "clear_all_events");
    }
    tray::sync(&app)
}

#[tauri::command]
async fn copy_to_clipboard(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    content_hash: String,
) -> Result<(), String> {
    debug_log!(
        "[copy_stack] copy_to_clipboard requested: content_hash={}",
        content_hash
    );

    let (event, restore_content_hash, move_restored_item_to_top) = {
        let db = state.db.lock().unwrap();
        let event = db
            .get_event_by_content_hash(&content_hash)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Event not found: {}", content_hash))?;
        let restore_content_hash = db
            .event_content_hash(&event)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Stored event has no restorable text".to_string())?;
        let move_restored_item_to_top = db
            .get_move_restored_item_to_top()
            .map_err(|e| e.to_string())?;
        (event, restore_content_hash, move_restored_item_to_top)
    };
    debug_log!(
        "[copy_stack] clipboard event loaded: content_hash={}",
        content_hash
    );

    if !move_restored_item_to_top {
        queue_restore_suppression(&state, restore_content_hash.clone());
        debug_log!(
            "[copy_stack] restore will preserve list order: content_hash={}",
            content_hash
        );
    }

    if let Err(error) = restore_event_to_clipboard(event) {
        clear_restore_suppression_if_matches(&state, &restore_content_hash);
        return Err(error);
    }
    debug_log!(
        "[copy_stack] clipboard event written to pasteboard: content_hash={}",
        content_hash
    );

    if move_restored_item_to_top {
        {
            let db = state.db.lock().unwrap();
            db.move_event_to_top(&content_hash)
                .map_err(|e| e.to_string())?;
            write_history_jsonl_if_enabled(&db, state.history_jsonl.as_ref(), "copy_to_clipboard");
        }
        debug_log!(
            "[copy_stack] clipboard event moved to top: content_hash={}",
            content_hash
        );

        tray::sync(&app_handle)?;
        tray::notify_history_changed(&app_handle)?;
        debug_log!(
            "[copy_stack] history/tray refresh notified: content_hash={}",
            content_hash
        );
    } else {
        debug_log!(
            "[copy_stack] clipboard event order unchanged: content_hash={}",
            content_hash
        );
    }

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

pub(crate) fn write_history_jsonl_if_enabled(
    db: &Database,
    config: Option<&HistoryJsonlConfig>,
    context: &str,
) {
    let Some(config) = config else {
        return;
    };

    match db.write_history_jsonl(config) {
        Ok(()) => {
            debug_log!(
                "[copy_stack] history JSONL written after {}: {}",
                context,
                config.path.display()
            );
        }
        Err(_error) => {
            debug_error!(
                "failed to write history JSONL after {} to {}: {}",
                context,
                config.path.display(),
                _error
            );
        }
    }
}

fn build_app_menu<R: Runtime>(
    app_handle: &AppHandle<R>,
    language: Language,
) -> tauri::Result<Menu<R>> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = language;
        return Menu::default(app_handle);
    }

    #[cfg(target_os = "macos")]
    {
        let strings = native_strings(language);
        let pkg_info = app_handle.package_info();
        let config = app_handle.config();
        let about_metadata = AboutMetadata {
            name: Some(pkg_info.name.clone()),
            version: Some(pkg_info.version.to_string()),
            copyright: config.bundle.copyright.clone(),
            authors: config
                .bundle
                .publisher
                .clone()
                .map(|publisher| vec![publisher]),
            ..Default::default()
        };

        let settings = MenuItemBuilder::with_id(OPEN_APP_SETTINGS_ID, strings.settings_ellipsis)
            .accelerator("CmdOrCtrl+,")
            .build(app_handle)?;

        let window_menu = Submenu::with_id_and_items(
            app_handle,
            WINDOW_SUBMENU_ID,
            strings.window,
            true,
            &[
                &PredefinedMenuItem::minimize(app_handle, Some(strings.minimize))?,
                &PredefinedMenuItem::maximize(app_handle, Some(strings.zoom))?,
                &PredefinedMenuItem::separator(app_handle)?,
                &PredefinedMenuItem::close_window(app_handle, Some(strings.close_window))?,
            ],
        )?;

        let help_menu =
            Submenu::with_id_and_items(app_handle, HELP_SUBMENU_ID, strings.help, true, &[])?;

        Menu::with_items(
            app_handle,
            &[
                &Submenu::with_items(
                    app_handle,
                    pkg_info.name.clone(),
                    true,
                    &[
                        &PredefinedMenuItem::about(
                            app_handle,
                            Some(strings.about_copy_stack),
                            Some(about_metadata),
                        )?,
                        &PredefinedMenuItem::separator(app_handle)?,
                        &settings,
                        &PredefinedMenuItem::separator(app_handle)?,
                        &PredefinedMenuItem::services(app_handle, Some(strings.services))?,
                        &PredefinedMenuItem::separator(app_handle)?,
                        &PredefinedMenuItem::hide(app_handle, Some(strings.hide_copy_stack))?,
                        &PredefinedMenuItem::hide_others(app_handle, Some(strings.hide_others))?,
                        &PredefinedMenuItem::separator(app_handle)?,
                        &PredefinedMenuItem::quit(app_handle, Some(strings.quit_copy_stack))?,
                    ],
                )?,
                &Submenu::with_items(
                    app_handle,
                    strings.file,
                    true,
                    &[&PredefinedMenuItem::close_window(
                        app_handle,
                        Some(strings.close_window),
                    )?],
                )?,
                &Submenu::with_items(
                    app_handle,
                    strings.edit,
                    true,
                    &[
                        &PredefinedMenuItem::undo(app_handle, Some(strings.undo))?,
                        &PredefinedMenuItem::redo(app_handle, Some(strings.redo))?,
                        &PredefinedMenuItem::separator(app_handle)?,
                        &PredefinedMenuItem::cut(app_handle, Some(strings.cut))?,
                        &PredefinedMenuItem::copy(app_handle, Some(strings.copy))?,
                        &PredefinedMenuItem::paste(app_handle, Some(strings.paste))?,
                        &PredefinedMenuItem::select_all(app_handle, Some(strings.select_all))?,
                    ],
                )?,
                &Submenu::with_items(
                    app_handle,
                    strings.view,
                    true,
                    &[&PredefinedMenuItem::fullscreen(
                        app_handle,
                        Some(strings.enter_full_screen),
                    )?],
                )?,
                &window_menu,
                &help_menu,
            ],
        )
    }
}

pub(crate) fn resolved_language<R: Runtime>(app: &AppHandle<R>) -> Result<Language, String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.get_language()
        .map(LanguagePreference::resolve)
        .map_err(|error| error.to_string())
}

fn replace_app_menu<R: Runtime>(app: &AppHandle<R>, language: Language) -> Result<(), String> {
    let menu = build_app_menu(app, language).map_err(|error| error.to_string())?;
    app.set_menu(menu)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn handle_app_menu_event<R: Runtime>(app: &AppHandle<R>, menu_id: &str) {
    if menu_id == OPEN_APP_SETTINGS_ID {
        if let Err(_error) = tray::show_settings_window(app) {
            debug_error!("app menu action failed: {}", _error);
        }
    }
}

#[tauri::command]
async fn get_event_by_content_hash(
    state: State<'_, AppState>,
    content_hash: String,
) -> Result<Option<ClipboardEvent>, String> {
    let db = state.db.lock().unwrap();
    db.get_clipboard_event_by_content_hash(&content_hash)
        .map_err(|e| e.to_string())
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
        write_history_jsonl_if_enabled(&db, state.history_jsonl.as_ref(), "set_max_items");
    }
    tray::sync(&app)?;
    tray::notify_history_changed(&app)
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

#[tauri::command]
async fn set_compact_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    compact_mode: bool,
) -> Result<(), String> {
    {
        let db = state.db.lock().unwrap();
        db.set_compact_mode(compact_mode)
            .map_err(|e| e.to_string())?;
    }
    tray::sync(&app)?;
    tray::notify_history_changed(&app)
}

#[tauri::command]
async fn set_language(
    app: AppHandle,
    state: State<'_, AppState>,
    language: String,
) -> Result<AppSettings, String> {
    let language = LanguagePreference::from_code(&language)
        .ok_or_else(|| format!("Unsupported language preference: {language}"))?;
    let settings = {
        let db = state.db.lock().unwrap();
        db.set_language(language)
            .map_err(|error| error.to_string())?;
        db.get_settings().map_err(|error| error.to_string())?
    };
    let resolved_language = language.resolve();

    if let Err(error) = replace_app_menu(&app, resolved_language) {
        debug_error!("failed to refresh localized application menu: {}", error);
    }
    if let Err(error) = tray::sync_window_titles(&app, resolved_language) {
        debug_error!("failed to refresh localized window titles: {}", error);
    }
    if let Err(error) = tray::sync(&app) {
        debug_error!("failed to refresh localized tray menu: {}", error);
    }
    if let Err(error) = tray::notify_language_changed(&app) {
        debug_error!("failed to notify frontend of language change: {}", error);
    }

    Ok(settings)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(rx: Receiver<Event>, startup_options: StartupOptions) {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .menu(|app| build_app_menu(app, Language::detect_system()))
        .on_menu_event(|app, event| {
            handle_app_menu_event(app, event.id().as_ref());
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(move |app| {
            debug_log!("[copy_stack] Tauri setup started");
            let app_handle = app.handle();
            let db = Database::new(&app_handle).expect("Failed to initialize database");
            debug_log!("[copy_stack] database initialized");

            // Update the app state with the database
            app.manage(AppState {
                db: Mutex::new(db),
                pending_restore_suppression: Mutex::new(None),
                history_jsonl: startup_options.history_jsonl.clone(),
            });

            let language =
                resolved_language(&app_handle).expect("Failed to resolve application language");
            replace_app_menu(&app_handle, language)
                .expect("Failed to initialize localized application menu");

            // Clean up old events on startup to respect max_items limit
            if let Ok(db) = app.state::<AppState>().db.lock() {
                let _ = db.cleanup_old_events();
                write_history_jsonl_if_enabled(
                    &db,
                    startup_options.history_jsonl.as_ref(),
                    "startup",
                );
            }

            tray::setup(&app_handle).expect("Failed to initialize tray");
            debug_log!("[copy_stack] tray initialized");

            // Handle incoming copy events
            let app_handle_clone = app_handle.clone();
            std::thread::spawn(move || {
                for event in rx {
                    debug_log!("[copy_stack] clipboard listener event received");
                    if !event.items.iter().any(|item| !item.data_list.is_empty()) {
                        debug_log!("[copy_stack] skipped clipboard event with no data");
                        continue;
                    }

                    let event_hash = {
                        let state = app_handle_clone.state::<AppState>();
                        let db = state.db.lock().unwrap();
                        db.event_content_hash(&event)
                            .map_err(|error| error.to_string())
                    };

                    let event_hash = match event_hash {
                        Ok(Some(event_hash)) => event_hash,
                        Ok(None) => {
                            debug_log!(
                                "[copy_stack] skipped unsupported or disallowed clipboard event"
                            );
                            continue;
                        }
                        Err(_error) => {
                            debug_error!("failed to classify clipboard item: {}", _error);
                            continue;
                        }
                    };

                    let state = app_handle_clone.state::<AppState>();
                    if should_skip_pending_restore_event(&state, &event_hash) {
                        debug_log!(
                            "[copy_stack] skipped restored clipboard event to preserve order"
                        );
                        continue;
                    }

                    debug_log!("[copy_stack] storing clipboard listener event");
                    let insert_result = {
                        let state = app_handle_clone.state::<AppState>();
                        let db = state.db.lock().unwrap();
                        let result = db.insert_event(&event).map_err(|error| error.to_string());
                        if matches!(result, Ok(true)) {
                            write_history_jsonl_if_enabled(
                                &db,
                                state.history_jsonl.as_ref(),
                                "clipboard insert",
                            );
                        }
                        result
                    };

                    match insert_result {
                        Ok(true) => {}
                        Ok(false) => {
                            debug_log!("[copy_stack] clipboard event filtered before persistence");
                            continue;
                        }
                        Err(_error) => {
                            debug_error!("failed to store clipboard item: {}", _error);
                            continue;
                        }
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
            get_event_by_content_hash,
            get_app_settings,
            set_max_items,
            set_show_in_menu_bar,
            set_move_restored_item_to_top,
            set_compact_mode,
            set_language
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
