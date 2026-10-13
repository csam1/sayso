use std::sync::Mutex;

use tauri::{AppHandle, Manager};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::audio::resample_to_16k;
use crate::model::{download_model_if_missing, model_path};
use crate::output::write_transcript;

pub struct WhisperState(pub Mutex<Option<WhisperContext>>);

impl WhisperState {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

pub fn ensure_model_loaded(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<WhisperState>();
    if state.0.lock().unwrap().is_some() {
        return Ok(());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to resolve app data dir: {e}"))?;
    let path = model_path(&app_data_dir);
    download_model_if_missing(&path)?;

    let context = WhisperContext::new_with_params(
        path.to_str().ok_or("model path is not valid UTF-8")?,
        WhisperContextParameters::default(),
    )
    .map_err(|e| format!("failed to load whisper model: {e}"))?;

    *state.0.lock().unwrap() = Some(context);
    Ok(())
}

#[tauri::command]
pub async fn transcribe_recording(
    app: AppHandle,
    samples: Vec<f32>,
    sample_rate: u32,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_model_loaded(&app)?;

        let resampled = resample_to_16k(&samples, sample_rate);
        if resampled.is_empty() {
            return Ok(String::new());
        }

        let state = app.state::<WhisperState>();
        let guard = state.0.lock().unwrap();
        let context = guard.as_ref().ok_or("model failed to load")?;

        let mut whisper_state = context
            .create_state()
            .map_err(|e| format!("failed to create whisper state: {e}"))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(4);
        params.set_translate(false);
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);

        whisper_state
            .full(params, &resampled)
            .map_err(|e| format!("transcription failed: {e}"))?;

        let mut text = String::new();
        for i in 0..whisper_state.full_n_segments() {
            if let Some(segment) = whisper_state.get_segment(i) {
                if let Ok(segment_text) = segment.to_str_lossy() {
                    if !is_non_speech_marker(&segment_text) {
                        text.push_str(&segment_text);
                    }
                }
            }
        }

        let trimmed = text.trim();
        if !trimmed.is_empty() {
            write_transcript(trimmed)?;
        }

        Ok(trimmed.to_string())
    })
    .await
    .map_err(|e| format!("transcription task panicked: {e}"))?
}

/// Whisper emits markers like `[BLANK_AUDIO]` or `(silence)` as plain text for non-speech audio.
fn is_non_speech_marker(segment: &str) -> bool {
    let segment = segment.trim();
    segment.is_empty()
        || (segment.starts_with('[') && segment.ends_with(']'))
        || (segment.starts_with('(') && segment.ends_with(')'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bracketed_and_parenthesised_markers_are_non_speech() {
        assert!(is_non_speech_marker("[BLANK_AUDIO]"));
        assert!(is_non_speech_marker(" [ Silence ] "));
        assert!(is_non_speech_marker("(silence)"));
        assert!(is_non_speech_marker("  "));
    }

    #[test]
    fn spoken_text_is_not_a_marker() {
        assert!(!is_non_speech_marker(" hello world"));
        assert!(!is_non_speech_marker(" call me (later) today"));
        assert!(!is_non_speech_marker("[note] then speech"));
    }
}
