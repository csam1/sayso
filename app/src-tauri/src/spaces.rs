//! Lets the overlay appear on every Space, including over full-screen apps.
//!
//! Without this, pressing the shortcut from a full-screen app shows the overlay on another Space,
//! where the webview counts as hidden and WebKit never starts the microphone.

use objc2_app_kit::{NSWindowCollectionBehavior, NSWindowStyleMask};
use tauri::{AppHandle, WebviewWindow};
use tauri_nspanel::{tauri_panel, ManagerExt, WebviewWindowExt};

tauri_panel! {
    panel!(OverlayPanel {
        config: {
            can_become_key_window: true,
            is_floating_panel: true
        }
    })
}

pub fn overlay_collection_behavior(current: NSWindowCollectionBehavior) -> NSWindowCollectionBehavior {
    current | NSWindowCollectionBehavior::CanJoinAllSpaces | NSWindowCollectionBehavior::FullScreenAuxiliary
}

/// Turns the overlay into a non-activating panel. macOS only lets panels like this onto another
/// app's full-screen Space. Must run on the main thread (e.g. in `setup`).
pub fn make_overlay_panel(window: &WebviewWindow) -> Result<(), String> {
    let panel = window.to_panel::<OverlayPanel>().map_err(|e| format!("failed to make overlay a panel: {e}"))?;
    panel
        .add_style_mask(NSWindowStyleMask::NonactivatingPanel)
        .map_err(|e| format!("failed to make overlay non-activating: {e}"))?;
    panel.set_collection_behavior(overlay_collection_behavior(panel.as_panel().collectionBehavior()));
    Ok(())
}

/// Shows the overlay on the current Space without activating Sayso. Returns false if it isn't a panel.
pub fn show_overlay(app: &AppHandle, label: &str) -> bool {
    match app.get_webview_panel(label) {
        Ok(panel) => {
            panel.show_and_make_key();
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_app_kit::NSWindowCollectionBehavior as B;

    #[test]
    fn adds_all_spaces_and_full_screen_flags() {
        let behavior = overlay_collection_behavior(B::Default);
        assert!(behavior.contains(B::CanJoinAllSpaces));
        assert!(behavior.contains(B::FullScreenAuxiliary));
    }

    #[test]
    fn keeps_existing_flags() {
        let behavior = overlay_collection_behavior(B::IgnoresCycle);
        assert!(behavior.contains(B::IgnoresCycle));
    }
}
