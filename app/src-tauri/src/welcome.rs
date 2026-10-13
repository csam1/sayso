use std::fs;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

const WELCOME_LABEL: &str = "welcome";

pub fn marker_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("welcome-seen")
}

pub fn should_show_welcome(app_data_dir: &Path) -> bool {
    !marker_path(app_data_dir).exists()
}

pub fn mark_welcome_seen(app_data_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(app_data_dir).map_err(|e| format!("failed to create app data dir: {e}"))?;
    fs::write(marker_path(app_data_dir), "").map_err(|e| format!("failed to write welcome marker: {e}"))
}

pub fn show_welcome(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WELCOME_LABEL) {
        window.show().ok();
        window.set_focus().ok();
    }
}

#[tauri::command]
pub fn welcome_done(app: AppHandle) {
    match app.path().app_data_dir() {
        Ok(dir) => {
            if let Err(e) = mark_welcome_seen(&dir) {
                eprintln!("{e}");
            }
        }
        Err(e) => eprintln!("failed to resolve app data dir: {e}"),
    }
    if let Some(window) = app.get_webview_window(WELCOME_LABEL) {
        window.hide().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fresh_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sayso-welcome-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn shows_welcome_when_no_marker_exists() {
        let dir = fresh_dir("no-marker");
        assert!(should_show_welcome(&dir));
    }

    #[test]
    fn hides_welcome_after_marked_seen() {
        let dir = fresh_dir("marked");
        mark_welcome_seen(&dir).expect("marking should succeed");
        assert!(!should_show_welcome(&dir));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mark_welcome_seen_creates_missing_directory() {
        let dir = fresh_dir("missing").join("nested");
        mark_welcome_seen(&dir).expect("marking should succeed");
        assert!(marker_path(&dir).exists());
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }
}
