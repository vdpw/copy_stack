// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use copy_event_listener::clipboard::ClipboardListener;

fn main() {
    let listener = ClipboardListener::new().with_interval(300);

    listener.run(move |event| {});

    copy_stack_lib::run( );
}
