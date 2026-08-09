use std::path::{Path, PathBuf};

pub const MODEL_FILE_NAME: &str = "ggml-base.en.bin";
pub const MODEL_DOWNLOAD_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";

pub fn model_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("models").join(MODEL_FILE_NAME)
}

pub fn download_model_if_missing(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }

    let parent = path
        .parent()
        .ok_or_else(|| "model path has no parent directory".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("failed to create model dir: {e}"))?;

    let response = ureq::get(MODEL_DOWNLOAD_URL)
        .call()
        .map_err(|e| format!("failed to download model: {e}"))?;

    let tmp_path = path.with_extension("bin.part");
    {
        let mut file = std::fs::File::create(&tmp_path)
            .map_err(|e| format!("failed to create model file: {e}"))?;
        std::io::copy(&mut response.into_reader(), &mut file)
            .map_err(|e| format!("failed to write model file: {e}"))?;
    }
    std::fs::rename(&tmp_path, path)
        .map_err(|e| format!("failed to finalize model file: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_path_joins_models_dir_and_file_name() {
        let dir = Path::new("/tmp/appdata");
        assert_eq!(
            model_path(dir),
            PathBuf::from("/tmp/appdata/models/ggml-base.en.bin")
        );
    }
}
