// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use copy_event_listener::clipboard::ClipboardListener;
use copy_event_listener::event::Event;
use copy_stack_lib::run;
use std::sync::mpsc;

fn main() {
    let (tx, rx) = mpsc::channel();

    // Start clipboard monitoring using copy_event_listener
    std::thread::spawn(move || {
        let listener = ClipboardListener::new().with_interval(500);

        listener.run(|event: Event| {
            // Send the event to the main thread
            let _ = tx.send(event);
        });
    });

    // Start the Tauri application
    run(rx);
}
