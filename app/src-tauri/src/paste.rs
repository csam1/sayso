use std::sync::Mutex;

/// PID of the app that was frontmost when the overlay opened — where the transcript gets pasted.
pub struct PasteTarget(pub Mutex<Option<i32>>);

impl PasteTarget {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

/// Returns the PID to paste into, or `None` when there is nothing to paste or nowhere sensible to paste it.
pub fn paste_target_for(text: &str, target: Option<i32>, own_pid: i32) -> Option<i32> {
    if text.trim().is_empty() {
        return None;
    }
    target.filter(|&pid| pid > 0 && pid != own_pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN_PID: i32 = 4242;

    #[test]
    fn pastes_into_remembered_app() {
        assert_eq!(paste_target_for("hello world", Some(100), OWN_PID), Some(100));
    }

    #[test]
    fn skips_empty_transcript() {
        assert_eq!(paste_target_for("", Some(100), OWN_PID), None);
    }

    #[test]
    fn skips_whitespace_only_transcript() {
        assert_eq!(paste_target_for("  \n\t ", Some(100), OWN_PID), None);
    }

    #[test]
    fn skips_when_no_app_was_remembered() {
        assert_eq!(paste_target_for("hello", None, OWN_PID), None);
    }

    #[test]
    fn skips_when_sayso_itself_was_frontmost() {
        assert_eq!(paste_target_for("hello", Some(OWN_PID), OWN_PID), None);
    }

    #[test]
    fn skips_invalid_pid() {
        assert_eq!(paste_target_for("hello", Some(-1), OWN_PID), None);
        assert_eq!(paste_target_for("hello", Some(0), OWN_PID), None);
    }

    #[test]
    fn paste_target_starts_empty() {
        assert_eq!(*PasteTarget::new().0.lock().unwrap(), None);
    }
}
