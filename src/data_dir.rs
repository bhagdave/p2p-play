/// Returns a path within the configured data directory for the given filename.
///
/// The data directory is controlled by the `DATA_DIR` environment variable,
/// which is set at startup when the `--data-dir` CLI flag is provided.
/// If `DATA_DIR` is not set, the filename is returned as-is, which resolves
/// to the current working directory (preserving the previous default behaviour).
pub fn get_data_path(filename: &str) -> String {
    build_data_path(std::env::var("DATA_DIR").ok().as_deref(), filename)
}

/// Inner helper used by `get_data_path` and by unit tests (which need to
/// inject a specific directory without touching the process environment).
pub(crate) fn build_data_path(data_dir: Option<&str>, filename: &str) -> String {
    if let Some(dir) = data_dir {
        std::path::Path::new(dir)
            .join(filename)
            .to_string_lossy()
            .into_owned()
    } else {
        filename.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_data_path_no_dir() {
        assert_eq!(build_data_path(None, "stories.db"), "stories.db");
        assert_eq!(build_data_path(None, "errors.log"), "errors.log");
        assert_eq!(build_data_path(None, "peer_key"), "peer_key");
    }

    #[test]
    fn test_build_data_path_with_dir() {
        let dir = std::path::Path::new("test-data");
        for filename in &["stories.db", "errors.log", "peer_key"] {
            let result = std::path::PathBuf::from(build_data_path(
                Some(dir.to_str().unwrap()),
                filename,
            ));
            assert_eq!(result, dir.join(filename));
        }
    }

    #[test]
    fn test_build_data_path_relative_dir() {
        let result_path =
            std::path::PathBuf::from(build_data_path(Some("relative/dir"), "stories.db"));
        let expected = std::path::PathBuf::from("relative")
            .join("dir")
            .join("stories.db");
        assert!(result_path.ends_with(&expected));
    }

    #[test]
    fn test_get_data_path_returns_filename_when_no_data_dir() {
        // If DATA_DIR is not set in this test's environment, the filename
        // is returned unchanged.  We cannot guarantee DATA_DIR is absent
        // in all CI environments, so we test the inner helper directly and
        // only assert on get_data_path() when DATA_DIR is absent.
        if std::env::var("DATA_DIR").is_err() {
            assert_eq!(get_data_path("stories.db"), "stories.db");
            assert_eq!(get_data_path("errors.log"), "errors.log");
        } else {
            // DATA_DIR is set (e.g. by another test or the environment); just
            // verify that get_data_path returns a path ending with the filename.
            let result = get_data_path("stories.db");
            assert!(result.ends_with("stories.db"));
        }
    }
}
