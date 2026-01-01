mod app;
mod config;
mod config_dialog;
mod input;
mod options_dialog;
mod persistence;
mod selection;
mod session_manager;
mod session_tree;
mod session_tree_view;
mod ssh;
mod tabs;
mod terminal;

use app::YasshApp;
use std::sync::Mutex;

// Store the debug console process so we can kill it on exit
static DEBUG_CONSOLE: Mutex<Option<std::process::Child>> = Mutex::new(None);

fn setup_debug_logging() {
    use std::process::Command;
    
    // Clear the debug log file
    let log_path = std::env::current_dir()
        .unwrap_or_default()
        .join("yassh_debug.log");
    let _ = std::fs::write(&log_path, "=== Yassh Debug Log ===\n\n");
    
    // Open a new console window that tails the log file
    #[cfg(windows)]
    {
        let log_path_str = log_path.to_string_lossy().to_string();
        // Get our own process ID so the debug console can monitor it
        let parent_pid = std::process::id();
        // Start a new PowerShell window that watches the log file and exits when parent dies
        let script = format!(
            "$host.UI.RawUI.WindowTitle = 'Yassh Debug Console'; \
             $parentPid = {}; \
             $job = Start-Job -ScriptBlock {{ Get-Content -Path '{}' -Wait }}; \
             while ($true) {{ \
                 if (-not (Get-Process -Id $parentPid -ErrorAction SilentlyContinue)) {{ \
                     Stop-Job $job; exit; \
                 }} \
                 Receive-Job $job; \
                 Start-Sleep -Milliseconds 100; \
             }}",
            parent_pid, log_path_str
        );
        if let Ok(child) = Command::new("cmd.exe")
            .args([
                "/c", "start", "Yassh Debug Console", "powershell.exe",
                "-NoProfile", "-Command", &script
            ])
            .spawn()
        {
            if let Ok(mut guard) = DEBUG_CONSOLE.lock() {
                *guard = Some(child);
            }
        }
    }
}

pub fn cleanup_debug_console() {
    #[cfg(windows)]
    {
        // The console monitors our PID and will exit automatically
        // But we also try to kill it explicitly just in case
        if let Ok(mut guard) = DEBUG_CONSOLE.lock() {
            if let Some(ref mut child) = *guard {
                let _ = child.kill();
            }
            *guard = None;
        }
    }
}

fn main() -> eframe::Result<()> {
    // Setup debug logging to a file and open debug console
    setup_debug_logging();
    
    env_logger::init();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("Yassh - SSH Terminal")
            .with_decorations(false),
        persist_window: false,
        ..Default::default()
    };
    let result = eframe::run_native(
        "Yassh",
        native_options,
        Box::new(|cc| Ok(Box::new(YasshApp::new(cc)))),
    );
    
    // Cleanup debug console on exit
    cleanup_debug_console();
    
    result
}

