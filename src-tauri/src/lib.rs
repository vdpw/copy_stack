// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

use std::thread;
use std::time::Duration;
use tauri::{Emitter, Window};

#[tauri::command]
fn start_listening(window: Window) {
    let window = window.clone();
    thread::spawn(move || {
        let mut i = 0;
        loop {
            thread::sleep(Duration::from_secs(1));
            window
                .emit("message", format!("Hello from Rust! {}", i))
                .unwrap();
            i += 1;
            if i > 10000 {
                i = 0;
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![start_listening])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
