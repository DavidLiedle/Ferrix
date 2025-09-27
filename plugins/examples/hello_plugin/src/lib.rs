use serde::{Deserialize, Serialize};

// Plugin manifest structure (matching Ferrix API)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub api_version: String,
    pub capabilities: Vec<String>,
    pub exports: Vec<String>,
}

static MANIFEST: &str = r#"{
    "name": "hello-ferrix",
    "version": "0.1.0",
    "author": "Ferrix Team",
    "description": "Example Hello World plugin for Ferrix",
    "homepage": null,
    "license": "MIT",
    "api_version": "0.1.0",
    "capabilities": ["SessionManagement", "StatusBar"],
    "exports": ["plugin_init", "plugin_cleanup", "get_manifest", "handle_event", "handle_command", "hook_post_session_create"]
}"#;

// WASM memory allocation helpers
#[no_mangle]
pub extern "C" fn allocate(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn deallocate(ptr: *mut u8, size: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, size);
    }
}

// Plugin exports

#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    log(0, "Hello plugin initialized!");
    0 // Success
}

#[no_mangle]
pub extern "C" fn plugin_cleanup() -> i32 {
    log(0, "Hello plugin cleaning up!");
    0 // Success
}

#[no_mangle]
pub extern "C" fn get_manifest() -> *const u8 {
    MANIFEST.as_ptr()
}

#[no_mangle]
pub extern "C" fn get_manifest_len() -> usize {
    MANIFEST.len()
}

#[no_mangle]
pub extern "C" fn handle_event(event_ptr: *const u8, event_len: usize) -> i32 {
    unsafe {
        let event_data = std::slice::from_raw_parts(event_ptr, event_len);
        let event_str = std::str::from_utf8_unchecked(event_data);

        // Parse event
        if event_str.contains("SessionCreated") {
            log(0, "Hello plugin: New session created!");

            // Update status bar
            let message = "Hello from plugin! 🔌";
            update_status_bar(message);
        } else if event_str.contains("SessionAttached") {
            log(0, "Hello plugin: Session attached!");
        }
    }

    0 // Success
}

#[no_mangle]
pub extern "C" fn handle_command(cmd_ptr: *const u8, cmd_len: usize) -> i32 {
    unsafe {
        let cmd_data = std::slice::from_raw_parts(cmd_ptr, cmd_len);
        let cmd_str = std::str::from_utf8_unchecked(cmd_data);

        log(0, &format!("Hello plugin received command: {}", cmd_str));

        // Handle specific commands
        if cmd_str.contains("ShowMessage") {
            return 0; // Command handled
        }
    }

    1 // Command not handled
}

#[no_mangle]
pub extern "C" fn hook_post_session_create(context_ptr: *const u8, context_len: usize) -> i32 {
    log(0, "Hello plugin: Post-session-create hook triggered!");

    // Send a welcome message
    show_message("Welcome to Ferrix! Session created successfully.", 0);

    0 // Success
}

// Ferrix API imports (these would be provided by the host)

extern "C" {
    fn ferrix_log(level: i32, ptr: *const u8, len: usize);
    fn ferrix_send_command(ptr: *const u8, len: usize) -> i32;
    fn ferrix_update_status_bar(ptr: *const u8, len: usize) -> i32;
    fn ferrix_show_message(msg_ptr: *const u8, msg_len: usize, level: i32) -> i32;
}

// Helper functions

fn log(level: i32, message: &str) {
    unsafe {
        ferrix_log(level, message.as_ptr(), message.len());
    }
}

fn update_status_bar(content: &str) {
    unsafe {
        ferrix_update_status_bar(content.as_ptr(), content.len());
    }
}

fn show_message(message: &str, level: i32) {
    unsafe {
        ferrix_show_message(message.as_ptr(), message.len(), level);
    }
}