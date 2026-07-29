// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use copy_stack_lib::{run, StartupOptions};

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
    let startup_options = StartupOptions::from_env_args().unwrap_or_else(|_| {
        eprintln!("[copy_stack] startup options were invalid; using safe defaults");
        StartupOptions {
            had_invalid_arguments: true,
            ..StartupOptions::default()
        }
    });

    debug_log!("[copy_stack] launching Tauri runtime");
    if let Err(error_code) = run(startup_options) {
        eprintln!("[copy_stack] application stopped: {error_code}");
        std::process::exit(1);
    }
}
