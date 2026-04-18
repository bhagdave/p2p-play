/// Returns the current Unix timestamp in seconds.
///
/// Centralised helper used throughout the codebase so the
/// `SystemTime::now().duration_since(UNIX_EPOCH)...` pattern
/// is not repeated in every module.
pub(crate) fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
