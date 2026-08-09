use std::fs;
use std::path::PathBuf;

pub fn output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("output")
        .join("transcripts")
}

pub fn write_transcript(text: &str) -> Result<PathBuf, String> {
    let dir = output_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("failed to create output dir: {e}"))?;

    let timestamp = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    let path = dir.join(format!("{timestamp}.txt"));
    fs::write(&path, text).map_err(|e| format!("failed to write transcript: {e}"))?;

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_dir_points_at_repo_root_output_transcripts() {
        assert!(output_dir().ends_with("output/transcripts"));
    }

    #[test]
    fn write_transcript_creates_file_with_given_text() {
        let path = write_transcript("hello world").expect("write should succeed");
        let contents = fs::read_to_string(&path).expect("file should exist");
        assert_eq!(contents, "hello world");
        fs::remove_file(&path).unwrap();
    }
}
