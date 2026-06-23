// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use copy_event_listener::clipboard::ClipboardListener;
use copy_event_listener::event::Event;
use copy_stack_lib::{run, StartupOptions};
use std::sync::mpsc;

macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            println!($($arg)*);
        }
    };
}

fn main() {
    debug_log!("[copy_stack] application starting");
    let startup_options = StartupOptions::from_env_args().unwrap_or_else(|error| {
        eprintln!("[copy_stack] failed to parse startup options: {}", error);
        std::process::exit(2);
    });

    let (tx, rx) = mpsc::channel();

    // Start clipboard monitoring using copy_event_listener
    std::thread::spawn(move || {
        debug_log!("[copy_stack] clipboard listener thread started");
        let listener = ClipboardListener::new().with_interval(500);

        listener.run(|event: Event| {
            debug_log!("[copy_stack] clipboard listener captured event");
            // Send the event to the main thread
            let _ = tx.send(event);
        });
    });

    // Start the Tauri application
    debug_log!("[copy_stack] launching Tauri runtime");
    run(rx, startup_options);
}
