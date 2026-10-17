//! Turns "points: buy milk … call the bank … renew passport" into a bulleted list.
//!
//! Lists are opt-in: only a transcript that starts with a spoken cue ("points", "bullet points", …)
//! is bulleted. Bullets break at pauses in the audio, or where the speaker says "next point".

/// Whisper timestamps are in centiseconds (10ms).
const SAMPLES_PER_CS: usize = 160; // at 16 kHz
const FRAME_SAMPLES: usize = 480; // 30ms
const MIN_PAUSE_CS: i64 = 60;
const MIN_THRESHOLD: f32 = 0.003;

const CUES: &[&str] = &[
    "bullet points",
    "in bullet points",
    "in points",
    "as points",
    "make a list",
    "points",
    "bullets",
    "list",
];

/// A spoken word with its start/end time in centiseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub start: i64,
    pub end: i64,
}

/// Quiet stretches of at least 600ms in 16 kHz audio, as (start, end) in centiseconds.
///
/// "Quiet" is relative to how loud this recording's speech is, so it adapts to mic gain.
pub fn find_pauses(samples: &[f32]) -> Vec<(i64, i64)> {
    let frame_rms: Vec<f32> = samples
        .chunks(FRAME_SAMPLES)
        .map(|frame| (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt())
        .collect();
    if frame_rms.is_empty() {
        return Vec::new();
    }

    let mut sorted = frame_rms.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let speech_level = sorted[(sorted.len() * 9 / 10).min(sorted.len() - 1)];
    let threshold = (speech_level * 0.2).max(MIN_THRESHOLD);

    let frame_cs = (FRAME_SAMPLES / SAMPLES_PER_CS) as i64;
    let mut pauses = Vec::new();
    let mut quiet_since: Option<usize> = None;
    for (i, &rms) in frame_rms.iter().chain(std::iter::once(&f32::MAX)).enumerate() {
        match (rms < threshold, quiet_since) {
            (true, None) => quiet_since = Some(i),
            (false, Some(start)) => {
                let (start_cs, end_cs) = (start as i64 * frame_cs, i as i64 * frame_cs);
                if end_cs - start_cs >= MIN_PAUSE_CS {
                    pauses.push((start_cs, end_cs));
                }
                quiet_since = None;
            }
            _ => {}
        }
    }
    pauses
}

/// Groups words into chunks, starting a new chunk wherever a pause falls between two words.
pub fn split_at_pauses(words: &[Word], pauses: &[(i64, i64)]) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut prev_mid: Option<i64> = None;

    for word in words {
        let mid = (word.start + word.end) / 2;
        if let Some(prev) = prev_mid {
            let paused = pauses.iter().any(|&(s, e)| {
                let pause_mid = (s + e) / 2;
                prev < pause_mid && pause_mid <= mid
            });
            if paused && !current.trim().is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
        }
        current.push_str(&word.text);
        prev_mid = Some(mid);
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
    chunks.into_iter().map(|c| c.trim().to_string()).collect()
}

/// Formats pause-separated chunks as a bulleted list if the speaker opened with a cue word;
/// otherwise returns `plain` unchanged.
pub fn format_transcript(plain: &str, chunks: &[String]) -> String {
    let Some((first, rest)) = chunks.split_first() else {
        return plain.to_string();
    };
    let Some(after_cue) = strip_cue(first) else {
        return plain.to_string();
    };

    let mut items: Vec<String> = std::iter::once(after_cue)
        .chain(rest.iter().cloned())
        .flat_map(|chunk| split_on_next_point(&chunk))
        .filter_map(|item| clean_item(&item))
        .collect();

    // No pause long enough between items: fall back to the commas Whisper wrote.
    if items.len() == 1 && items[0].contains([',', ';']) {
        items = items[0].split([',', ';']).filter_map(clean_item).collect();
    }

    if items.is_empty() {
        return plain.to_string();
    }
    items.iter().map(|item| format!("- {item}")).collect::<Vec<_>>().join("\n")
}

/// Returns the chunk with the cue removed, or `None` if it doesn't open with a cue.
/// The cue must stand alone (followed by a pause or punctuation), so "list the files" stays prose.
fn strip_cue(chunk: &str) -> Option<String> {
    let lower = chunk.to_lowercase();
    for cue in CUES {
        let Some(after) = lower.strip_prefix(cue) else {
            continue;
        };
        let after_trimmed = after.trim_start();
        let stands_alone = after_trimmed.is_empty()
            || after_trimmed.starts_with([':', ',', '.', '-', ';', '!']);
        if stands_alone {
            let rest = &chunk[chunk.len() - after.len()..];
            return Some(rest.trim_start_matches(|c: char| c.is_whitespace() || ":,.-;!".contains(c)).to_string());
        }
    }
    None
}

/// Splits on spoken "next point" / "new point", dropping the phrase.
fn split_on_next_point(chunk: &str) -> Vec<String> {
    let words: Vec<&str> = chunk.split_whitespace().collect();
    let bare = |w: &str| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();

    let mut items = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let is_marker = i + 1 < words.len()
            && matches!(bare(words[i]).as_str(), "next" | "new")
            && bare(words[i + 1]) == "point";
        if is_marker {
            items.push(current.join(" "));
            current.clear();
            i += 2;
        } else {
            current.push(words[i]);
            i += 1;
        }
    }
    items.push(current.join(" "));
    items
}

fn clean_item(item: &str) -> Option<String> {
    let item = item.trim().trim_matches(|c: char| ",;.:-".contains(c)).trim();
    let item = item.strip_prefix("and ").or_else(|| item.strip_prefix("And ")).unwrap_or(item).trim();
    let mut chars = item.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunks(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    fn word(text: &str, start: i64, end: i64) -> Word {
        Word { text: text.to_string(), start, end }
    }

    #[test]
    fn cue_with_pauses_becomes_bullets() {
        let out = format_transcript(
            "Points. Buy milk. Call the bank. Renew passport.",
            &chunks(&["Points.", "Buy milk.", "Call the bank.", "Renew passport."]),
        );
        assert_eq!(out, "- Buy milk\n- Call the bank\n- Renew passport");
    }

    #[test]
    fn cue_in_same_chunk_as_first_item() {
        let out = format_transcript("", &chunks(&["Bullet points: buy milk", "call the bank"]));
        assert_eq!(out, "- Buy milk\n- Call the bank");
    }

    #[test]
    fn no_cue_leaves_text_unchanged() {
        let plain = "Buy milk. Call the bank.";
        assert_eq!(format_transcript(plain, &chunks(&["Buy milk.", "Call the bank."])), plain);
    }

    #[test]
    fn cue_word_starting_a_sentence_is_not_a_cue() {
        let plain = "List the files in this folder";
        assert_eq!(format_transcript(plain, &chunks(&[plain])), plain);
        let plain = "Points of interest are marked";
        assert_eq!(format_transcript(plain, &chunks(&[plain])), plain);
    }

    #[test]
    fn next_point_forces_a_break() {
        let out = format_transcript("", &chunks(&["Points.", "buy milk next point call the bank. New point, renew passport"]));
        assert_eq!(out, "- Buy milk\n- Call the bank\n- Renew passport");
    }

    #[test]
    fn single_item_falls_back_to_commas() {
        let out = format_transcript("", &chunks(&["Points: buy milk, call the bank, and renew passport."]));
        assert_eq!(out, "- Buy milk\n- Call the bank\n- Renew passport");
    }

    #[test]
    fn cue_with_nothing_after_returns_plain() {
        assert_eq!(format_transcript("Points.", &chunks(&["Points."])), "Points.");
    }

    #[test]
    fn words_split_where_a_pause_falls_between_them() {
        let words = vec![
            word(" Buy", 0, 30),
            word(" milk", 30, 60),
            word(" call", 150, 180),
            word(" the", 180, 190),
            word(" bank", 190, 220),
        ];
        assert_eq!(split_at_pauses(&words, &[(65, 145)]), vec!["Buy milk", "call the bank"]);
        assert_eq!(split_at_pauses(&words, &[]), vec!["Buy milk call the bank"]);
    }

    #[test]
    fn finds_only_long_quiet_stretches() {
        let loud = |cs: usize| vec![0.3_f32; cs * SAMPLES_PER_CS];
        let quiet = |cs: usize| vec![0.0_f32; cs * SAMPLES_PER_CS];
        let samples: Vec<f32> = [loud(90), quiet(90), loud(90), quiet(30), loud(90)].concat();

        let pauses = find_pauses(&samples);
        assert_eq!(pauses.len(), 1, "{pauses:?}");
        let (start, end) = pauses[0];
        assert!((87..=93).contains(&start) && (177..=183).contains(&end), "{pauses:?}");
    }

    #[test]
    fn silence_only_has_no_crash() {
        assert!(find_pauses(&[]).is_empty());
        assert_eq!(find_pauses(&vec![0.0; 16_000]).len(), 1);
    }
}
