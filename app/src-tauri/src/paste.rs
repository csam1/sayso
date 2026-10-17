use std::sync::Mutex;

/// PID of the app that was frontmost when the overlay opened — where the transcript gets pasted.
pub struct PasteTarget(pub Mutex<Option<i32>>);

impl PasteTarget {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

/// What to do with the remembered app once recording stops.
#[derive(Debug, PartialEq, Eq)]
pub enum StopAction {
    /// Paste the transcript into this app.
    Paste(i32),
    /// Nothing to paste, but hand focus back so keystrokes don't go to Sayso's hidden window.
    Refocus(i32),
    Nothing,
}

pub fn stop_action(text: &str, target: Option<i32>, own_pid: i32) -> StopAction {
    match target.filter(|&pid| pid > 0 && pid != own_pid) {
        None => StopAction::Nothing,
        Some(pid) if text.trim().is_empty() => StopAction::Refocus(pid),
        Some(pid) => StopAction::Paste(pid),
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::thread;
    use std::time::{Duration, Instant};

    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, KeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use objc2_app_kit::{
        NSApplicationActivationOptions, NSPasteboard, NSPasteboardTypeString,
        NSRunningApplication, NSWorkspace,
    };
    use objc2_foundation::NSString;

    const ACTIVATION_TIMEOUT: Duration = Duration::from_millis(1000);
    const ACTIVATION_POLL: Duration = Duration::from_millis(20);
    // Give the re-activated app a moment to restore keyboard focus to its text field.
    const SETTLE_DELAY: Duration = Duration::from_millis(50);

    pub fn frontmost_app_pid() -> Option<i32> {
        let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
        let pid = app.processIdentifier();
        (pid > 0).then_some(pid)
    }

    pub fn paste_into(pid: i32, text: &str) -> Result<(), String> {
        set_clipboard(text)?;
        activate_app(pid).map_err(|e| format!("{e}; transcript left on clipboard"))?;
        thread::sleep(SETTLE_DELAY);

        send_cmd_v()
    }

    /// Brings `pid` to the front and waits until macOS reports it frontmost.
    pub fn activate_app(pid: i32) -> Result<(), String> {
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
            .ok_or("target app is no longer running")?;
        app.activateWithOptions(NSApplicationActivationOptions::empty());
        wait_until_frontmost(pid)
    }

    /// The app's display name, e.g. "Notes".
    pub fn app_name(pid: i32) -> Option<String> {
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;
        Some(app.localizedName()?.to_string())
    }

    fn set_clipboard(text: &str) -> Result<(), String> {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let ok = pasteboard
            .setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString });
        if ok {
            Ok(())
        } else {
            Err("failed to write transcript to clipboard".into())
        }
    }

    fn wait_until_frontmost(pid: i32) -> Result<(), String> {
        let deadline = Instant::now() + ACTIVATION_TIMEOUT;
        while Instant::now() < deadline {
            if frontmost_app_pid() == Some(pid) {
                return Ok(());
            }
            thread::sleep(ACTIVATION_POLL);
        }
        Err("target app did not come to the front".into())
    }

    fn send_cmd_v() -> Result<(), String> {
        for keydown in [true, false] {
            let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
                .map_err(|_| "failed to create keyboard event source")?;
            let event = CGEvent::new_keyboard_event(source, KeyCode::ANSI_V, keydown)
                .map_err(|_| "failed to create ⌘V key event")?;
            event.set_flags(CGEventFlags::CGEventFlagCommand);
            event.post(CGEventTapLocation::HID);
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub use macos::{activate_app, app_name, frontmost_app_pid, paste_into};

#[cfg(not(target_os = "macos"))]
pub fn app_name(_pid: i32) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN_PID: i32 = 4242;

    #[test]
    fn pastes_into_remembered_app() {
        assert_eq!(stop_action("hello world", Some(100), OWN_PID), StopAction::Paste(100));
    }

    #[test]
    fn refocuses_remembered_app_when_transcript_is_empty() {
        assert_eq!(stop_action("", Some(100), OWN_PID), StopAction::Refocus(100));
    }

    #[test]
    fn refocuses_remembered_app_when_transcript_is_whitespace() {
        assert_eq!(stop_action("  \n\t ", Some(100), OWN_PID), StopAction::Refocus(100));
    }

    #[test]
    fn does_nothing_when_no_app_was_remembered() {
        assert_eq!(stop_action("hello", None, OWN_PID), StopAction::Nothing);
    }

    #[test]
    fn does_nothing_when_sayso_itself_was_frontmost() {
        assert_eq!(stop_action("hello", Some(OWN_PID), OWN_PID), StopAction::Nothing);
    }

    #[test]
    fn does_nothing_for_invalid_pid() {
        assert_eq!(stop_action("hello", Some(-1), OWN_PID), StopAction::Nothing);
        assert_eq!(stop_action("hello", Some(0), OWN_PID), StopAction::Nothing);
    }

    #[test]
    fn paste_target_starts_empty() {
        assert_eq!(*PasteTarget::new().0.lock().unwrap(), None);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::*;
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};

    fn clipboard_text() -> Option<String> {
        NSPasteboard::generalPasteboard()
            .stringForType(unsafe { NSPasteboardTypeString })
            .map(|s| s.to_string())
    }

    #[test]
    fn frontmost_app_pid_is_positive() {
        let pid = frontmost_app_pid().expect("some app should be frontmost");
        assert!(pid > 0);
    }

    #[test]
    fn frontmost_app_has_a_name() {
        let pid = frontmost_app_pid().expect("some app should be frontmost");
        assert!(app_name(pid).is_some_and(|name| !name.is_empty()));
    }

    #[test]
    fn unknown_pid_has_no_name() {
        assert_eq!(app_name(-1), None);
    }

    #[test]
    #[ignore = "overwrites the system clipboard; run with `cargo test -- --ignored`"]
    fn paste_into_missing_app_errors_but_leaves_text_on_clipboard() {
        let err = paste_into(999_999, "sayso clipboard test").unwrap_err();
        assert!(err.contains("no longer running"), "unexpected error: {err}");
        assert_eq!(clipboard_text().as_deref(), Some("sayso clipboard test"));
    }
}
