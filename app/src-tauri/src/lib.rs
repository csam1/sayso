mod audio;
mod model;
mod output;
mod paste;
mod transcribe;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WindowEvent,
};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};
use paste::PasteTarget;
use transcribe::{ensure_model_loaded, transcribe_recording, WhisperState};

const OVERLAY_LABEL: &str = "overlay";

fn toggle_overlay(app: &AppHandle) {
    let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
        return;
    };

    let is_visible = window.is_visible().unwrap_or(false);
    if is_visible {
        // Second press while recording: the frontend owns the audio, so it stops, transcribes and pastes.
        app.emit("stop-requested", ()).ok();
    } else {
        remember_paste_target(app);
        window.show().ok();
        window.set_focus().ok();
        app.emit("recording-state-changed", true).ok();
    }
}

fn remember_paste_target(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        *app.state::<PasteTarget>().0.lock().unwrap() = paste::frontmost_app_pid();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[tauri::command]
fn stop_recording(app: AppHandle, transcript: Option<String>) {
    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        window.hide().ok();
    }
    app.emit("recording-state-changed", false).ok();

    let target = app.state::<PasteTarget>().0.lock().unwrap().take();
    let text = transcript.unwrap_or_default();
    let Some(pid) = paste::paste_target_for(&text, target, std::process::id() as i32) else {
        return;
    };

    #[cfg(target_os = "macos")]
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = paste::paste_into(pid, text.trim()) {
            eprintln!("paste failed: {e}");
        }
    });
    #[cfg(not(target_os = "macos"))]
    let _ = pid;
}

#[cfg(target_os = "macos")]
fn prime_macos_permissions(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if !tauri_plugin_macos_permissions::check_accessibility_permission().await {
            tauri_plugin_macos_permissions::request_accessibility_permission().await;
        }

        if !tauri_plugin_macos_permissions::check_microphone_permission().await {
            let _ = tauri_plugin_macos_permissions::request_microphone_permission().await;
        }

        let _ = app;
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let toggle_shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space);

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::POSITION)
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut == &toggle_shortcut && event.state() == ShortcutState::Pressed {
                        toggle_overlay(app);
                    }
                })
                .build(),
        );

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_plugin_macos_permissions::init());
    }

    builder
        .manage(WhisperState::new())
        .manage(PasteTarget::new())
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            app.global_shortcut().register(toggle_shortcut)?;

            let toggle_item =
                MenuItem::with_id(app, "toggle", "Toggle Recording", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle_item, &quit_item])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle" => toggle_overlay(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            #[cfg(target_os = "macos")]
            prime_macos_permissions(app.handle());

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match tauri::async_runtime::spawn_blocking(move || ensure_model_loaded(&app_handle))
                    .await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => eprintln!("model preload failed: {e}"),
                    Err(e) => eprintln!("model preload task panicked: {e}"),
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                window.hide().ok();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![stop_recording, transcribe_recording])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
