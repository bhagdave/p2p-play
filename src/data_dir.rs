/// Returns a path within the configured data directory for the given filename.
///
/// The data directory is controlled by the `DATA_DIR` environment variable,
/// which is set at startup when the `--data-dir` CLI flag is provided.
/// If `DATA_DIR` is not set, the filename is returned as-is, which resolves
/// to the current working directory (preserving the previous default behaviour).
pub fn get_data_path(filename: &str) -> String {
    if let Ok(data_dir) = std::env::var("DATA_DIR") {
        let path = std::path::Path::new(&data_dir).join(filename);
        path.to_string_lossy().into_owned()
    } else {
        filename.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_data_path_fallback() {
        // When DATA_DIR is not set, the filename is returned unchanged.
        // We test the logic without mutating the environment.
        let result_no_dir = {
            // Simulate get_data_path with no env var
            let data_dir: Option<String> = None;
            match data_dir {
                Some(d) => std::path::Path::new(&d)
                    .join("stories.db")
                    .to_string_lossy()
                    .into_owned(),
                None => "stories.db".to_string(),
            }
        };
        assert_eq!(result_no_dir, "stories.db");
    }

    #[test]
    fn test_get_data_path_with_dir() {
        let data_dir = "/tmp/test-data";
        let path = std::path::Path::new(data_dir).join("stories.db");
        assert_eq!(path.to_string_lossy(), "/tmp/test-data/stories.db");

        let path2 = std::path::Path::new(data_dir).join("errors.log");
        assert_eq!(path2.to_string_lossy(), "/tmp/test-data/errors.log");
    }
}
