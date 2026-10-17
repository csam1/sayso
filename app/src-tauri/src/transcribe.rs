use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::audio::resample_to_16k;
use crate::model::{download_model_if_missing, model_path};
use crate::history::{record, HistoryState};
use crate::paste::{app_name, PasteTarget};
use crate::points::{find_pauses, format_transcript, split_at_pauses, Word};

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

        let duration_ms = match sample_rate {
            0 => 0,
            rate => (samples.len() as u64 * 1000 / rate as u64) as i64,
        };
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
        params.set_token_timestamps(true);

        whisper_state
            .full(params, &resampled)
            .map_err(|e| format!("transcription failed: {e}"))?;

        let first_special_token = context.token_eot();
        let mut text = String::new();
        let mut words: Vec<Word> = Vec::new();
        for i in 0..whisper_state.full_n_segments() {
            let Some(segment) = whisper_state.get_segment(i) else {
                continue;
            };
            let Ok(segment_text) = segment.to_str_lossy() else {
                continue;
            };
            if is_non_speech_marker(&segment_text) {
                continue;
            }
            text.push_str(&segment_text);

            for t in 0..segment.n_tokens() {
                let Some(token) = segment.get_token(t) else {
                    continue;
                };
                if token.token_id() >= first_special_token {
                    continue;
                }
                let Ok(piece) = token.to_str_lossy() else {
                    continue;
                };
                let data = token.token_data();
                // Sub-word pieces without a leading space continue the previous word.
                match words.last_mut() {
                    Some(word) if !piece.starts_with(' ') => {
                        word.text.push_str(&piece);
                        word.end = data.t1;
                    }
                    _ => words.push(Word { text: piece.into_owned(), start: data.t0, end: data.t1 }),
                }
            }
        }

        let plain = text.trim();
        let chunks = split_at_pauses(&words, &find_pauses(&resampled));
        let formatted = format_transcript(plain, &chunks);
        save_to_history(&app, &formatted, duration_ms);

        Ok(formatted)
    })
    .await
    .map_err(|e| format!("transcription task panicked: {e}"))?
}

/// Best effort: a history failure is logged and never stops the transcript from being pasted.
fn save_to_history(app: &AppHandle, text: &str, duration_ms: i64) {
    // Peek, don't take: `stop_recording` still needs the target to paste into.
    let target = *app.state::<PasteTarget>().0.lock().unwrap();
    let app_name = target.and_then(app_name);

    let state = app.state::<HistoryState>();
    let guard = state.0.lock().unwrap();
    let Some(store) = guard.as_ref() else {
        return;
    };
    match record(store, text, chrono::Utc::now().timestamp_millis(), duration_ms, app_name) {
        Ok(true) => {
            app.emit("history-changed", ()).ok();
        }
        Ok(false) => {}
        Err(e) => eprintln!("{e}"),
    }
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
