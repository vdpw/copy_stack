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

mod command_error;
pub mod event;
mod history_mirror;
mod i18n;
mod lifecycle;
mod pasteboard_protocol;
mod private_fs;
mod resource_policy;
mod startup;
mod store;
mod tray;

pub use startup::StartupOptions;

use crate::command_error::{
    CommandError, CommandResult, DiagnosticLog, ErrorCode, Operation, SafeDiagnostic,
};
use crate::history_mirror::{HistoryMirror, HistoryMirrorConfig};
use crate::i18n::{native_strings, Language, LanguagePreference};
use crate::lifecycle::AutostartBackend;
use crate::pasteboard_protocol::{assess_event, prepare_event_for_restore};
use crate::resource_policy::prepare_capture_event;
use crate::store::{AppSettings, Database, HistoryDetail, HistoryPage};
use copy_event_listener::clipboard::ClipboardListener;
use copy_event_listener::event::Event;
use serde::Serialize;
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{
    AboutMetadata, Menu, MenuItemBuilder, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID,
    WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Emitter, Manager, RunEvent, Runtime, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const RESTORE_SUPPRESSION_TTL: Duration = Duration::from_secs(5);
const HISTORY_MIRROR_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const CAPTURE_TRAY_REFRESH_DEBOUNCE: Duration = Duration::from_millis(100);
const OPEN_APP_SETTINGS_ID: &str = "app-menu::open-settings";
pub(crate) const CAPTURE_REJECTED_EVENT: &str = "capture-rejected";
pub(crate) const APP_OPERATION_ERROR_EVENT: &str = "app-operation-error";

// State to hold the database
pub struct AppState {
    pub(crate) db: Mutex<Database>,
    pub(crate) pending_restore_suppression: Mutex<Option<PendingRestoreSuppression>>,
    pub(crate) history_mirror: Option<HistoryMirror>,
    tray_refresh: Option<TrayRefreshScheduler>,
    diagnostics: DiagnosticLog,
}

#[derive(Default)]
pub struct StartupStatus {
    latest_error: Mutex<Option<CommandError>>,
}

pub(crate) struct PendingRestoreSuppression {
    content_hash: String,
    created_at: Instant,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct CaptureRejectedNotice {
    code: &'static str,
    size_bucket: &'static str,
}

enum TrayRefreshMessage {
    Refresh,
    Shutdown,
}

struct TrayRefreshScheduler {
    sender: mpsc::Sender<TrayRefreshMessage>,
}

impl TrayRefreshScheduler {
    fn start(app: AppHandle) -> Result<Self, &'static str> {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("copy-stack-tray-refresh".to_string())
            .spawn(move || {
                run_tray_refresh_worker(receiver, || {
                    if tray::sync(&app).is_err() {
                        report_capture_tray_refresh_failure(&app);
                    }
                });
            })
            .map_err(|_| "TRAY_REFRESH_THREAD_START_FAILED")?;
        Ok(Self { sender })
    }

    fn schedule(&self) -> Result<(), ()> {
        self.sender
            .send(TrayRefreshMessage::Refresh)
            .map_err(|_| ())
    }

    fn shutdown(&self) {
        let _ = self.sender.send(TrayRefreshMessage::Shutdown);
    }
}

fn run_tray_refresh_worker(
    receiver: mpsc::Receiver<TrayRefreshMessage>,
    mut refresh: impl FnMut(),
) {
    while let Ok(message) = receiver.recv() {
        if matches!(message, TrayRefreshMessage::Shutdown) {
            return;
        }

        let deadline = Instant::now()
            .checked_add(CAPTURE_TRAY_REFRESH_DEBOUNCE)
            .unwrap_or_else(Instant::now);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match receiver.recv_timeout(remaining) {
                Ok(TrayRefreshMessage::Refresh) => {}
                Ok(TrayRefreshMessage::Shutdown) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
        refresh();
    }
}

fn record_command_error(state: &AppState, error: CommandError) -> CommandError {
    let _ = state.diagnostics.record(&error);
    error
}

fn publish_startup_error<R: Runtime>(app: &AppHandle<R>, error: CommandError) {
    if let Some(status) = app.try_state::<StartupStatus>() {
        if let Ok(mut latest_error) = status.latest_error.lock() {
            *latest_error = Some(error.clone());
        }
    }
    if let Some(state) = app.try_state::<AppState>() {
        let _ = state.diagnostics.record(&error);
    }
    let _ = app.emit(APP_OPERATION_ERROR_EVENT, &error);
}

pub(crate) fn report_restore_post_processing_failure<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
) {
    let error = CommandError::new(
        ErrorCode::RestorePostProcessingFailed,
        Operation::RestoreClipboard,
        false,
    );
    let _ = state.diagnostics.record(&error);
    let _ = app.emit(APP_OPERATION_ERROR_EVENT, &error);
}

pub(crate) fn report_tray_operation_failure<R: Runtime>(app: &AppHandle<R>) {
    let error = CommandError::new(ErrorCode::StateUnavailable, Operation::LoadHistory, true);
    if let Some(state) = app.try_state::<AppState>() {
        let _ = state.diagnostics.record(&error);
    }
    let _ = app.emit(APP_OPERATION_ERROR_EVENT, &error);
}

fn report_capture_tray_refresh_failure<R: Runtime>(app: &AppHandle<R>) {
    let error = CommandError::new(
        ErrorCode::StateUnavailable,
        Operation::CaptureClipboard,
        true,
    );
    if let Some(state) = app.try_state::<AppState>() {
        let _ = state.diagnostics.record(&error);
    }
    let _ = app.emit(APP_OPERATION_ERROR_EVENT, &error);
}

fn database_error(state: &AppState, operation: Operation) -> CommandError {
    record_command_error(state, CommandError::database(operation))
}

fn state_error(state: &AppState, operation: Operation) -> CommandError {
    record_command_error(state, CommandError::state(operation))
}

fn database_unavailable(state: &AppState, operation: Operation) -> CommandError {
    record_command_error(
        state,
        CommandError::new(ErrorCode::DatabaseUnavailable, operation, true),
    )
}

fn history_cursor_is_valid(cursor: &str) -> bool {
    let mut parts = cursor.splitn(3, ':');
    parts.next() == Some("v1")
        && parts
            .next()
            .is_some_and(|timestamp| timestamp.parse::<i64>().is_ok())
        && parts.next().is_some_and(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
}

fn schedule_history_mirror(state: &AppState) -> CommandResult<()> {
    let Some(mirror) = state.history_mirror.as_ref() else {
        return Ok(());
    };
    mirror.schedule_refresh().map(|_| ()).map_err(|_| {
        record_command_error(
            state,
            CommandError::new(
                ErrorCode::HistoryMirrorFailed,
                Operation::WriteHistoryMirror,
                true,
            ),
        )
    })
}

pub(crate) fn schedule_history_mirror_for_tray(state: &AppState) -> Result<(), String> {
    schedule_history_mirror(state).map_err(|_| "HISTORY_MIRROR_FAILED".to_string())
}

#[tauri::command]
fn get_copy_events_page(
    state: State<'_, AppState>,
    cursor: Option<String>,
    page_size: Option<usize>,
) -> CommandResult<HistoryPage> {
    if cursor
        .as_deref()
        .is_some_and(|cursor| !history_cursor_is_valid(cursor))
    {
        return Err(record_command_error(
            &state,
            CommandError::new(
                ErrorCode::InvalidHistoryCursor,
                Operation::LoadHistory,
                false,
            ),
        ));
    }
    let db = state
        .db
        .lock()
        .map_err(|_| database_unavailable(&state, Operation::LoadHistory))?;
    db.get_history_page(cursor.as_deref(), page_size)
        .map_err(|_| database_error(&state, Operation::LoadHistory))
}

#[tauri::command]
fn get_history_detail(
    state: State<'_, AppState>,
    content_hash: String,
) -> CommandResult<HistoryDetail> {
    let (seed, compact_mode) = {
        let db = state
            .db
            .lock()
            .map_err(|_| database_unavailable(&state, Operation::LoadHistoryDetail))?;
        let seed = db
            .get_history_detail_seed(&content_hash)
            .map_err(|_| database_error(&state, Operation::LoadHistoryDetail))?
            .ok_or_else(|| {
                record_command_error(
                    &state,
                    CommandError::new(
                        ErrorCode::HistoryItemNotFound,
                        Operation::LoadHistoryDetail,
                        false,
                    ),
                )
            })?;
        let compact_mode = db
            .get_compact_mode()
            .map_err(|_| database_error(&state, Operation::LoadHistoryDetail))?;
        (seed, compact_mode)
    };

    Database::build_history_detail(seed, compact_mode)
        .map_err(|_| database_error(&state, Operation::LoadHistoryDetail))
}

#[tauri::command]
fn delete_copy_event(
    app: AppHandle,
    state: State<'_, AppState>,
    content_hash: String,
) -> CommandResult<()> {
    {
        let db = state
            .db
            .lock()
            .map_err(|_| database_unavailable(&state, Operation::DeleteHistory))?;
        db.delete_event(&content_hash)
            .map_err(|_| database_error(&state, Operation::DeleteHistory))?;
    }
    schedule_history_mirror(&state)?;
    tray::sync(&app).map_err(|_| state_error(&state, Operation::DeleteHistory))
}

#[tauri::command]
fn clear_all_events(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    {
        let db = state
            .db
            .lock()
            .map_err(|_| database_unavailable(&state, Operation::ClearHistory))?;
        db.clear_all_events()
            .map_err(|_| database_error(&state, Operation::ClearHistory))?;
    }
    schedule_history_mirror(&state)?;
    tray::sync(&app).map_err(|_| state_error(&state, Operation::ClearHistory))
}

#[tauri::command]
fn copy_to_clipboard(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    content_hash: String,
) -> CommandResult<()> {
    let (seed, move_restored_item_to_top) = {
        let db = state
            .db
            .lock()
            .map_err(|_| database_unavailable(&state, Operation::RestoreClipboard))?;
        let seed = db
            .get_restore_seed(&content_hash)
            .map_err(|_| database_error(&state, Operation::RestoreClipboard))?
            .ok_or_else(|| {
                record_command_error(
                    &state,
                    CommandError::new(
                        ErrorCode::HistoryItemNotFound,
                        Operation::RestoreClipboard,
                        false,
                    ),
                )
            })?;
        let move_restored_item_to_top = db
            .get_move_restored_item_to_top()
            .map_err(|_| database_error(&state, Operation::RestoreClipboard))?;
        (seed, move_restored_item_to_top)
    };

    let restore_content_hash = seed.content_hash.clone();
    let source_bundle_id = seed.source_bundle_id.clone();
    let is_remote_clipboard = seed.is_remote_clipboard;
    let event = seed
        .into_event()
        .map_err(|_| database_error(&state, Operation::RestoreClipboard))?
        .ok_or_else(|| {
            record_command_error(
                &state,
                CommandError::new(
                    ErrorCode::HistoryItemNotFound,
                    Operation::RestoreClipboard,
                    false,
                ),
            )
        })?;
    let event = prepare_event_for_restore(event, source_bundle_id.as_deref(), is_remote_clipboard)
        .map_err(|_| {
            record_command_error(
                &state,
                CommandError::new(
                    ErrorCode::ClipboardWriteFailed,
                    Operation::RestoreClipboard,
                    true,
                ),
            )
        })?;

    if !move_restored_item_to_top {
        let mut pending = state
            .pending_restore_suppression
            .lock()
            .map_err(|_| state_error(&state, Operation::RestoreClipboard))?;
        *pending = Some(PendingRestoreSuppression {
            content_hash: restore_content_hash.clone(),
            created_at: Instant::now(),
        });
    }

    if restore_event_to_clipboard(event).is_err() {
        clear_restore_suppression_if_matches(&state, &restore_content_hash);
        return Err(record_command_error(
            &state,
            CommandError::new(
                ErrorCode::ClipboardWriteFailed,
                Operation::RestoreClipboard,
                true,
            ),
        ));
    }

    if move_restored_item_to_top {
        let post_processing_result = (|| -> Result<(), ()> {
            {
                let db = state.db.lock().map_err(|_| ())?;
                db.move_event_to_top(&content_hash).map_err(|_| ())?;
            }
            state
                .history_mirror
                .as_ref()
                .map(|mirror| mirror.schedule_refresh().map(|_| ()))
                .transpose()
                .map_err(|_| ())?;
            tray::sync(&app_handle).map_err(|_| ())?;
            tray::notify_history_changed(&app_handle).map_err(|_| ())
        })();
        if post_processing_result.is_err() {
            report_restore_post_processing_failure(&app_handle, &state);
        }
    }

    Ok(())
}

pub(crate) fn restore_event_to_clipboard(event: Event) -> Result<(), String> {
    debug_log!("[copy_stack] writing clipboard event to pasteboard");
    ClipboardListener::new()
        .set_clipboard_event(event)
        .map_err(|_| "CLIPBOARD_WRITE_FAILED".to_string())
}

pub(crate) fn queue_restore_suppression(state: &AppState, content_hash: String) {
    if let Ok(mut pending) = state.pending_restore_suppression.lock() {
        *pending = Some(PendingRestoreSuppression {
            content_hash,
            created_at: Instant::now(),
        });
    }
}

pub(crate) fn clear_restore_suppression_if_matches(state: &AppState, content_hash: &str) {
    if let Ok(mut pending) = state.pending_restore_suppression.lock() {
        if pending
            .as_ref()
            .is_some_and(|suppression| suppression.content_hash == content_hash)
        {
            *pending = None;
        }
    }
}

fn should_skip_pending_restore_event(state: &AppState, content_hash: &str) -> bool {
    should_consume_pending_restore(
        &state.pending_restore_suppression,
        content_hash,
        Instant::now(),
    )
}

fn should_consume_pending_restore(
    pending_restore_suppression: &Mutex<Option<PendingRestoreSuppression>>,
    content_hash: &str,
    now: Instant,
) -> bool {
    let Ok(mut pending) = pending_restore_suppression.lock() else {
        return false;
    };
    let Some(suppression) = pending.as_ref() else {
        return false;
    };

    if now.saturating_duration_since(suppression.created_at) > RESTORE_SUPPRESSION_TTL {
        *pending = None;
        return false;
    }

    if suppression.content_hash != content_hash {
        return false;
    }

    *pending = None;
    true
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
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "APP_STATE_UNAVAILABLE".to_string())?;
    let db = state
        .db
        .lock()
        .map_err(|_| "DATABASE_STATE_UNAVAILABLE".to_string())?;
    db.get_language()
        .map(LanguagePreference::resolve)
        .map_err(|_| "LANGUAGE_READ_FAILED".to_string())
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
            report_tray_operation_failure(app);
            debug_error!("app menu action failed: {}", _error);
        }
    }
}

#[tauri::command]
fn get_app_settings(state: State<'_, AppState>) -> CommandResult<AppSettings> {
    let db = state
        .db
        .lock()
        .map_err(|_| database_unavailable(&state, Operation::LoadSettings))?;
    db.get_settings()
        .map_err(|_| database_error(&state, Operation::LoadSettings))
}

#[tauri::command]
fn get_startup_error(state: State<'_, StartupStatus>) -> CommandResult<Option<CommandError>> {
    state
        .latest_error
        .lock()
        .map(|error| error.clone())
        .map_err(|_| CommandError::new(ErrorCode::StateUnavailable, Operation::Startup, true))
}

#[tauri::command]
fn get_safe_diagnostics(state: State<'_, AppState>) -> CommandResult<Vec<SafeDiagnostic>> {
    state
        .diagnostics
        .snapshot()
        .map_err(|error| record_command_error(&state, error))
}

struct TauriAutostartBackend<'a> {
    app: &'a AppHandle,
}

impl AutostartBackend for TauriAutostartBackend<'_> {
    fn is_enabled(&self) -> Result<bool, ()> {
        self.app.autolaunch().is_enabled().map_err(|_| ())
    }

    fn enable(&self) -> Result<(), ()> {
        self.app.autolaunch().enable().map_err(|_| ())
    }

    fn disable(&self) -> Result<(), ()> {
        self.app.autolaunch().disable().map_err(|_| ())
    }
}

#[tauri::command]
fn get_autostart_status(app: AppHandle, state: State<'_, AppState>) -> CommandResult<bool> {
    let backend = TauriAutostartBackend { app: &app };
    lifecycle::read_autostart_enabled(&backend).map_err(|_| {
        record_command_error(
            &state,
            CommandError::new(
                ErrorCode::AutostartUnavailable,
                Operation::UpdateAutostart,
                true,
            ),
        )
    })
}

#[tauri::command]
fn set_autostart_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> CommandResult<bool> {
    let backend = TauriAutostartBackend { app: &app };
    lifecycle::set_autostart_enabled(&backend, enabled).map_err(|error| {
        let code = if error == lifecycle::LifecycleError::AutostartVerificationFailed {
            ErrorCode::AutostartVerificationFailed
        } else {
            ErrorCode::AutostartUnavailable
        };
        record_command_error(
            &state,
            CommandError::new(code, Operation::UpdateAutostart, true),
        )
    })
}

#[tauri::command]
fn set_max_items(app: AppHandle, state: State<'_, AppState>, max_items: u32) -> CommandResult<()> {
    if !(1..=1_000).contains(&max_items) {
        return Err(record_command_error(
            &state,
            CommandError::new(ErrorCode::InvalidSetting, Operation::UpdateSettings, false),
        ));
    }
    {
        let db = state
            .db
            .lock()
            .map_err(|_| database_unavailable(&state, Operation::UpdateSettings))?;
        db.set_max_items(max_items)
            .and_then(|_| db.cleanup_old_events())
            .map_err(|_| database_error(&state, Operation::UpdateSettings))?;
    }
    schedule_history_mirror(&state)?;
    tray::sync(&app).map_err(|_| state_error(&state, Operation::UpdateSettings))?;
    tray::notify_history_changed(&app).map_err(|_| state_error(&state, Operation::UpdateSettings))
}

#[tauri::command]
fn set_max_history_bytes(
    app: AppHandle,
    state: State<'_, AppState>,
    max_history_bytes: u64,
) -> CommandResult<()> {
    if !(16 * 1024 * 1024..=4 * 1024 * 1024 * 1024).contains(&max_history_bytes) {
        return Err(record_command_error(
            &state,
            CommandError::new(ErrorCode::InvalidSetting, Operation::UpdateSettings, false),
        ));
    }
    {
        let db = state
            .db
            .lock()
            .map_err(|_| database_unavailable(&state, Operation::UpdateSettings))?;
        db.set_max_history_bytes(max_history_bytes)
            .and_then(|_| db.cleanup_old_events())
            .map_err(|_| database_error(&state, Operation::UpdateSettings))?;
    }
    schedule_history_mirror(&state)?;
    tray::sync(&app).map_err(|_| state_error(&state, Operation::UpdateSettings))?;
    tray::notify_history_changed(&app).map_err(|_| state_error(&state, Operation::UpdateSettings))
}

#[tauri::command]
fn set_show_in_menu_bar(
    app: AppHandle,
    state: State<'_, AppState>,
    show_in_menu_bar: bool,
) -> CommandResult<()> {
    {
        let db = state
            .db
            .lock()
            .map_err(|_| database_unavailable(&state, Operation::UpdateSettings))?;
        db.set_show_in_menu_bar(show_in_menu_bar)
            .map_err(|_| database_error(&state, Operation::UpdateSettings))?;
    }
    tray::sync(&app).map_err(|_| state_error(&state, Operation::UpdateSettings))
}

#[tauri::command]
fn set_move_restored_item_to_top(
    state: State<'_, AppState>,
    move_restored_item_to_top: bool,
) -> CommandResult<()> {
    let db = state
        .db
        .lock()
        .map_err(|_| database_unavailable(&state, Operation::UpdateSettings))?;
    db.set_move_restored_item_to_top(move_restored_item_to_top)
        .map_err(|_| database_error(&state, Operation::UpdateSettings))
}

#[tauri::command]
fn set_compact_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    compact_mode: bool,
) -> CommandResult<()> {
    {
        let db = state
            .db
            .lock()
            .map_err(|_| database_unavailable(&state, Operation::UpdateSettings))?;
        db.set_compact_mode(compact_mode)
            .map_err(|_| database_error(&state, Operation::UpdateSettings))?;
    }
    schedule_history_mirror(&state)?;
    tray::sync(&app).map_err(|_| state_error(&state, Operation::UpdateSettings))?;
    tray::notify_history_changed(&app).map_err(|_| state_error(&state, Operation::UpdateSettings))
}

#[tauri::command]
fn set_language(
    app: AppHandle,
    state: State<'_, AppState>,
    language: String,
) -> CommandResult<AppSettings> {
    let language = LanguagePreference::from_code(&language).ok_or_else(|| {
        record_command_error(
            &state,
            CommandError::new(ErrorCode::InvalidSetting, Operation::UpdateSettings, false),
        )
    })?;
    let settings = {
        let db = state
            .db
            .lock()
            .map_err(|_| database_unavailable(&state, Operation::UpdateSettings))?;
        db.set_language(language)
            .map_err(|_| database_error(&state, Operation::UpdateSettings))?;
        db.get_settings()
            .map_err(|_| database_error(&state, Operation::UpdateSettings))?
    };
    let resolved_language = language.resolve();

    replace_app_menu(&app, resolved_language)
        .map_err(|_| state_error(&state, Operation::UpdateSettings))?;
    tray::sync_window_titles(&app, resolved_language)
        .map_err(|_| state_error(&state, Operation::UpdateSettings))?;
    tray::sync(&app).map_err(|_| state_error(&state, Operation::UpdateSettings))?;
    tray::notify_language_changed(&app)
        .map_err(|_| state_error(&state, Operation::UpdateSettings))?;

    Ok(settings)
}

fn start_clipboard_event_pipeline(app_handle: AppHandle) -> Result<(), &'static str> {
    let (tx, rx) = mpsc::channel::<Event>();
    let event_app_handle = app_handle.clone();

    std::thread::Builder::new()
        .name("copy-stack-event-store".to_string())
        .spawn(move || {
            for event in rx {
                debug_log!("[copy_stack] clipboard listener event received");
                if !event.items.iter().any(|item| !item.data_list.is_empty()) {
                    debug_log!("[copy_stack] skipped clipboard event with no data");
                    continue;
                }

                let state = event_app_handle.state::<AppState>();
                if !assess_event(&event).should_record() {
                    debug_log!("[copy_stack] skipped clipboard event by protocol policy");
                    continue;
                }

                let event = match prepare_capture_event(event) {
                    Ok(prepared) => prepared.event,
                    Err(rejection) => {
                        let error = CommandError::new(
                            ErrorCode::CaptureRejected,
                            Operation::CaptureClipboard,
                            false,
                        );
                        let _ = state.diagnostics.record(&error);
                        let _ = event_app_handle.emit(
                            CAPTURE_REJECTED_EVENT,
                            CaptureRejectedNotice {
                                code: rejection.kind.code(),
                                size_bucket: rejection.size_bucket.code(),
                            },
                        );
                        debug_log!("[copy_stack] rejected clipboard event by resource policy");
                        continue;
                    }
                };

                let compact_mode = match state.db.lock() {
                    Ok(db) => match db.get_compact_mode() {
                        Ok(compact_mode) => compact_mode,
                        Err(_) => {
                            let _ = state
                                .diagnostics
                                .record(&CommandError::database(Operation::CaptureClipboard));
                            debug_error!("[copy_stack] clipboard settings unavailable");
                            continue;
                        }
                    },
                    Err(_) => {
                        let _ = state
                            .diagnostics
                            .record(&CommandError::state(Operation::CaptureClipboard));
                        debug_error!("[copy_stack] database state unavailable");
                        continue;
                    }
                };

                let prepared = match Database::prepare_history_event(&event, compact_mode) {
                    Ok(Some(prepared)) => prepared,
                    Ok(None) => {
                        debug_log!("[copy_stack] skipped unsupported clipboard event");
                        continue;
                    }
                    Err(_) => {
                        let _ = state
                            .diagnostics
                            .record(&CommandError::database(Operation::CaptureClipboard));
                        debug_error!("[copy_stack] clipboard classification failed");
                        continue;
                    }
                };
                let event_hash = prepared.content_hash().to_string();

                if should_skip_pending_restore_event(&state, &event_hash) {
                    debug_log!("[copy_stack] skipped restored clipboard event to preserve order");
                    continue;
                }

                debug_log!("[copy_stack] storing clipboard listener event");
                let insert_result = {
                    let db = match state.db.lock() {
                        Ok(db) => db,
                        Err(_) => {
                            let _ = state
                                .diagnostics
                                .record(&CommandError::state(Operation::CaptureClipboard));
                            debug_error!("[copy_stack] database state unavailable");
                            continue;
                        }
                    };
                    db.insert_prepared_event(prepared)
                };

                match insert_result {
                    Ok(true) => {
                        if schedule_history_mirror(&state).is_err() {
                            debug_error!("[copy_stack] history mirror scheduling failed");
                        }
                    }
                    Ok(false) => {
                        debug_log!("[copy_stack] clipboard event filtered before persistence");
                        continue;
                    }
                    Err(_) => {
                        let _ = state
                            .diagnostics
                            .record(&CommandError::database(Operation::CaptureClipboard));
                        debug_error!("[copy_stack] clipboard persistence failed");
                        continue;
                    }
                }

                let tray_refresh_scheduled = state.tray_refresh.as_ref().map_or_else(
                    || tray::sync(&event_app_handle).map_err(|_| ()),
                    |refresh| refresh.schedule(),
                );
                if tray_refresh_scheduled.is_err() {
                    report_capture_tray_refresh_failure(&event_app_handle);
                    debug_error!("[copy_stack] tray refresh scheduling failed");
                }
                if tray::notify_history_changed(&event_app_handle).is_err() {
                    report_capture_tray_refresh_failure(&event_app_handle);
                    debug_error!("[copy_stack] history notification failed");
                }
            }
        })
        .map_err(|_| "CLIPBOARD_EVENT_THREAD_START_FAILED")?;

    std::thread::Builder::new()
        .name("copy-stack-listener".to_string())
        .spawn(move || {
            debug_log!("[copy_stack] clipboard listener thread started");
            let listener = ClipboardListener::new().with_interval(500);
            listener.run(move |event: Event| {
                debug_log!("[copy_stack] clipboard listener captured event");
                let _ = tx.send(event);
            });
        })
        .map_err(|_| "CLIPBOARD_LISTENER_THREAD_START_FAILED")?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(startup_options: StartupOptions) -> Result<(), String> {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Err(_error) = lifecycle::activate_main_window(app) {
                debug_error!(
                    "[copy_stack] second-instance activation failed: {}",
                    _error.code()
                );
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![startup::AUTOSTART_LAUNCH_FLAG]),
        ))
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
            app.manage(StartupStatus::default());
            if startup_options.had_invalid_arguments {
                publish_startup_error(
                    &app_handle,
                    CommandError::new(ErrorCode::StartupFailed, Operation::Startup, false),
                );
            }

            if lifecycle::apply_initial_window_visibility(
                &app_handle,
                startup_options.launched_at_login,
            )
            .is_err()
            {
                publish_startup_error(
                    &app_handle,
                    CommandError::new(ErrorCode::StartupFailed, Operation::Startup, false),
                );
            }

            let db = match Database::new(&app_handle) {
                Ok(db) => db,
                Err(_) => {
                    publish_startup_error(
                        &app_handle,
                        CommandError::new(
                            ErrorCode::DatabaseUnavailable,
                            Operation::Startup,
                            false,
                        ),
                    );
                    return Ok(());
                }
            };
            debug_log!("[copy_stack] database initialized");

            if db.cleanup_old_events().is_err() {
                publish_startup_error(
                    &app_handle,
                    CommandError::new(
                        ErrorCode::DatabaseOperationFailed,
                        Operation::Startup,
                        false,
                    ),
                );
            }
            let history_mirror = match startup_options.history_jsonl.as_ref() {
                Some(config) => match db.history_mirror_database_path() {
                    Ok(database_path) => match HistoryMirror::start_database(
                        HistoryMirrorConfig::new(config.path.clone(), config.max_data_bytes),
                        database_path,
                    ) {
                        Ok(mirror) => Some(mirror),
                        Err(_) => {
                            publish_startup_error(
                                &app_handle,
                                CommandError::new(
                                    ErrorCode::HistoryMirrorFailed,
                                    Operation::Startup,
                                    false,
                                ),
                            );
                            None
                        }
                    },
                    Err(_) => {
                        publish_startup_error(
                            &app_handle,
                            CommandError::new(
                                ErrorCode::HistoryMirrorFailed,
                                Operation::Startup,
                                false,
                            ),
                        );
                        None
                    }
                },
                None => None,
            };
            let history_mirror_enabled = history_mirror.is_some();
            let tray_refresh = match TrayRefreshScheduler::start(app_handle.clone()) {
                Ok(scheduler) => Some(scheduler),
                Err(_) => {
                    publish_startup_error(
                        &app_handle,
                        CommandError::new(ErrorCode::StartupFailed, Operation::Startup, false),
                    );
                    None
                }
            };

            app.manage(AppState {
                db: Mutex::new(db),
                pending_restore_suppression: Mutex::new(None),
                history_mirror,
                tray_refresh,
                diagnostics: DiagnosticLog::default(),
            });
            if let (Some(status), Some(state)) = (
                app_handle.try_state::<StartupStatus>(),
                app_handle.try_state::<AppState>(),
            ) {
                if let Ok(latest_error) = status.latest_error.lock() {
                    if let Some(error) = latest_error.as_ref() {
                        let _ = state.diagnostics.record(error);
                    }
                }
            }

            let language = match resolved_language(&app_handle) {
                Ok(language) => language,
                Err(_) => {
                    publish_startup_error(
                        &app_handle,
                        CommandError::new(
                            ErrorCode::DatabaseOperationFailed,
                            Operation::Startup,
                            false,
                        ),
                    );
                    Language::detect_system()
                }
            };
            if replace_app_menu(&app_handle, language).is_err() {
                publish_startup_error(
                    &app_handle,
                    CommandError::new(ErrorCode::StartupFailed, Operation::Startup, false),
                );
            }

            if history_mirror_enabled {
                let state = app.state::<AppState>();
                if schedule_history_mirror(&state).is_err() {
                    publish_startup_error(
                        &app_handle,
                        CommandError::new(
                            ErrorCode::HistoryMirrorFailed,
                            Operation::Startup,
                            false,
                        ),
                    );
                }
            }

            if tray::setup(&app_handle).is_err() {
                publish_startup_error(
                    &app_handle,
                    CommandError::new(ErrorCode::StartupFailed, Operation::Startup, false),
                );
            } else {
                debug_log!("[copy_stack] tray initialized");
            }

            if start_clipboard_event_pipeline(app_handle.clone()).is_err() {
                publish_startup_error(
                    &app_handle,
                    CommandError::new(ErrorCode::StartupFailed, Operation::Startup, false),
                );
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_startup_error,
            get_copy_events_page,
            get_history_detail,
            delete_copy_event,
            clear_all_events,
            copy_to_clipboard,
            get_app_settings,
            get_safe_diagnostics,
            get_autostart_status,
            set_autostart_enabled,
            set_max_items,
            set_max_history_bytes,
            set_show_in_menu_bar,
            set_move_restored_item_to_top,
            set_compact_mode,
            set_language
        ])
        .build(tauri::generate_context!())
        .map_err(|_| "APP_BUILD_FAILED".to_string())?;

    app.run(|app_handle, event| {
        if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
            if let Some(state) = app_handle.try_state::<AppState>() {
                if let Some(tray_refresh) = state.tray_refresh.as_ref() {
                    tray_refresh.shutdown();
                }
                if let Some(mirror) = state.history_mirror.as_ref() {
                    if mirror.shutdown(HISTORY_MIRROR_SHUTDOWN_TIMEOUT).is_err() {
                        let error = CommandError::new(
                            ErrorCode::HistoryMirrorFailed,
                            Operation::WriteHistoryMirror,
                            true,
                        );
                        let _ = state.diagnostics.record(&error);
                        debug_error!("[copy_stack] history mirror shutdown failed");
                    }
                }
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod lib_tests {
    use super::*;
    use crate::pasteboard_protocol::{REMOTE_CLIPBOARD_TYPE, SOURCE_TYPE};
    use copy_event_listener::event::{Data, Item};

    #[test]
    fn rapid_capture_tray_refreshes_are_coalesced() {
        let (sender, receiver) = mpsc::channel();
        let (refreshed_sender, refreshed_receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            run_tray_refresh_worker(receiver, || {
                let _ = refreshed_sender.send(());
            });
        });

        for _ in 0..20 {
            sender
                .send(TrayRefreshMessage::Refresh)
                .expect("refresh should queue");
        }
        refreshed_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("coalesced refresh should run");
        assert!(
            refreshed_receiver
                .recv_timeout(CAPTURE_TRAY_REFRESH_DEBOUNCE * 2)
                .is_err(),
            "one rapid capture burst must build the tray only once"
        );

        sender
            .send(TrayRefreshMessage::Shutdown)
            .expect("shutdown should queue");
        worker.join().expect("tray refresh worker should stop");
    }

    #[test]
    fn restore_protocol_metadata_keeps_suppression_hash_stable_in_both_modes() {
        let original = Event {
            items: vec![Item {
                data_list: vec![
                    Data {
                        r#type: "public.utf8-plain-text".to_string(),
                        data: b"synthetic restore suppression".to_vec(),
                    },
                    Data {
                        r#type: "public.rtf".to_string(),
                        data: br"{\rtf1 synthetic restore suppression}".to_vec(),
                    },
                ],
            }],
        };
        let restored =
            prepare_event_for_restore(original.clone(), Some("com.example.synthetic"), true)
                .expect("restore metadata should canonicalize");
        assert_eq!(
            restored
                .items
                .iter()
                .flat_map(|item| item.data_list.iter())
                .filter(|data| data.r#type == SOURCE_TYPE)
                .count(),
            1
        );
        assert_eq!(
            restored
                .items
                .iter()
                .flat_map(|item| item.data_list.iter())
                .filter(|data| data.r#type == REMOTE_CLIPBOARD_TYPE)
                .count(),
            1
        );

        for compact_mode in [false, true] {
            let original_hash = Database::prepare_history_event(&original, compact_mode)
                .expect("original event should prepare")
                .expect("original event should be recordable")
                .content_hash()
                .to_string();
            let restored_hash = Database::prepare_history_event(&restored, compact_mode)
                .expect("restored event should prepare")
                .expect("restored event should be recordable")
                .content_hash()
                .to_string();
            assert_eq!(
                original_hash, restored_hash,
                "canonical protocol markers changed the suppression identity"
            );

            let now = Instant::now();
            let pending = Mutex::new(Some(PendingRestoreSuppression {
                content_hash: original_hash,
                created_at: now,
            }));
            assert!(should_consume_pending_restore(
                &pending,
                &restored_hash,
                now
            ));
            assert!(
                pending
                    .lock()
                    .expect("pending suppression should remain readable")
                    .is_none(),
                "a matching restored event should consume suppression exactly once"
            );
            assert!(!should_consume_pending_restore(
                &pending,
                &restored_hash,
                now
            ));
        }
    }
}
