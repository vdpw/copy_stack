// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod event;
mod store;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;
use tauri::{Emitter, Window};

static LISTENER_STARTED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
fn start_listening(window: Window) {
    if LISTENER_STARTED.load(Ordering::SeqCst) {
        println!("Listener already started");
        return;
    }
    LISTENER_STARTED.store(true, Ordering::SeqCst);

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
pub fn run(rx: Receiver<event::Event>) {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let e = app.emit;
            thread::spawn(move || {
                for event in rx {
                    app.emit("new-copy", event).unwrap();
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
