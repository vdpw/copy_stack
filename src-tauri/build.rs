fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_startup_error",
            "get_copy_events_page",
            "get_history_detail",
            "delete_copy_event",
            "clear_all_events",
            "copy_to_clipboard",
            "get_app_settings",
            "get_safe_diagnostics",
            "get_autostart_status",
            "set_autostart_enabled",
            "set_max_items",
            "set_max_history_bytes",
            "set_show_in_menu_bar",
            "set_menu_bar_item_limit",
            "set_move_restored_item_to_top",
            "set_compact_mode",
            "set_language",
        ]),
    ))
    .expect("failed to build Copy Stack with scoped command permissions");
}
