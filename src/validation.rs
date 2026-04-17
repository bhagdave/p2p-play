/// Valid types for WASM parameters.  These are the accepted types for
/// capability-exchange parameter declarations.  Stored as a module-level
/// constant so the list is easy to extend and is not rebuilt on every call
/// to [`ContentValidator::validate_wasm_param_type`].
const WASM_PARAM_TYPES: &[&str] = &["string", "bytes", "json", "int", "float", "bool", "file"];

pub struct ContentLimits;

impl ContentLimits {
    pub const STORY_NAME_MAX: usize = 100;
    pub const STORY_HEADER_MAX: usize = 200;
    pub const STORY_BODY_MAX: usize = 10_000;
    pub const CHANNEL_NAME_MAX: usize = 50;
    /// Separate from `STORY_HEADER_MAX` so channel and story limits can
    /// diverge independently in the future.
    pub const CHANNEL_DESCRIPTION_MAX: usize = 200;
    pub const PEER_NAME_MAX: usize = 30;
    pub const DIRECT_MESSAGE_MAX: usize = 1_000;
    pub const NODE_DESCRIPTION_MAX: usize = 2_000;

    // WASM offering limits
    pub const WASM_OFFERING_NAME_MAX: usize = 100;
    pub const WASM_OFFERING_DESCRIPTION_MAX: usize = 500;
    pub const WASM_IPFS_CID_MAX: usize = 100;
    pub const WASM_VERSION_MAX: usize = 20;
    pub const WASM_PARAM_NAME_MAX: usize = 50;
    pub const WASM_PARAM_TYPE_MAX: usize = 20;
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    TooLong {
        max_length: usize,
        actual_length: usize,
    },
    Empty,
    InvalidCharacters {
        invalid_chars: Vec<char>,
    },
    ContainsControlCharacters,
    ContainsAnsiEscapes,
    ContainsBinaryData,
    InvalidFormat {
        expected: String,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::TooLong {
                max_length,
                actual_length,
            } => {
                write!(
                    f,
                    "Content too long: {actual_length} characters (max: {max_length})"
                )
            }
            ValidationError::Empty => write!(f, "Content cannot be empty"),
            ValidationError::InvalidCharacters { invalid_chars } => {
                let chars_str = invalid_chars.iter().collect::<String>();
                write!(
                    f,
                    "Contains invalid characters: '{chars_str}'. Valid characters: alphanumeric, hyphens (-), underscores (_), dots (.)"
                )
            }
            ValidationError::ContainsControlCharacters => {
                write!(f, "Contains terminal control characters")
            }
            ValidationError::ContainsAnsiEscapes => {
                write!(f, "Contains ANSI escape sequences")
            }
            ValidationError::ContainsBinaryData => {
                write!(f, "Contains binary data or null bytes")
            }
            ValidationError::InvalidFormat { expected } => {
                write!(f, "Invalid format, expected: {expected}")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

pub type ValidationResult<T> = Result<T, ValidationError>;

pub struct ContentSanitizer;

impl ContentSanitizer {
    pub fn strip_ansi_escapes(text: &str) -> String {
        let mut result = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1b'
                && let Some(&next_ch) = chars.peek()
            {
                match next_ch {
                    '[' => {
                        chars.next(); // consume '['
                        #[allow(clippy::while_let_on_iterator)]
                        while let Some(seq_ch) = chars.next() {
                            if seq_ch.is_ascii_alphabetic() || seq_ch == '~' {
                                break;
                            }
                        }
                    }
                    ']' => {
                        chars.next(); // consume ']'
                        #[allow(clippy::while_let_on_iterator)]
                        while let Some(seq_ch) = chars.next() {
                            if seq_ch == '\x07' {
                                break;
                            } else if seq_ch == '\x1b' && chars.peek() == Some(&'\\') {
                                chars.next(); // consume '\'
                                break;
                            }
                        }
                    }
                    '(' | ')' | '*' | '+' => {
                        chars.next(); // consume intermediate
                        chars.next(); // consume final char
                    }
                    _ => {
                        if next_ch.is_ascii_alphabetic() {
                            chars.next(); // consume the character
                        }
                    }
                }
                continue;
            }
            result.push(ch);
        }

        result
    }

    /// Returns `true` for whitespace characters that are allowed to pass
    /// through both the sanitizer and the strict checker: space, tab,
    /// newline, and carriage return.
    #[inline]
    fn is_allowed_whitespace(ch: char) -> bool {
        matches!(ch, ' ' | '\t' | '\n' | '\r')
    }

    pub fn strip_control_characters(text: &str) -> String {
        text.chars()
            .filter(|&ch| !ch.is_control() || Self::is_allowed_whitespace(ch))
            .collect()
    }

    pub fn strip_binary_data(text: &str) -> String {
        text.chars()
            .filter(|&ch| ch != '\0' && (!ch.is_control() || Self::is_allowed_whitespace(ch)))
            .collect()
    }

    /// Strip ANSI escapes and all non-whitespace control characters.
    /// The ANSI state machine requires its own pass, so this method performs
    /// two allocations: one for ANSI removal and one for the control-char
    /// filter.
    pub fn sanitize_for_display(text: &str) -> String {
        let no_ansi = Self::strip_ansi_escapes(text);
        no_ansi
            .chars()
            .filter(|&ch| !ch.is_control() || Self::is_allowed_whitespace(ch))
            .collect()
    }

    pub fn sanitize_for_storage(text: &str) -> String {
        let text = Self::strip_ansi_escapes(text);
        // For storage, we allow newlines and tabs but remove other control chars
        text.chars()
            .filter(|&ch| ch != '\0' && (!ch.is_control() || matches!(ch, '\n' | '\r' | '\t')))
            .collect()
    }

    // ========================================================================
    // Strict checks — return errors instead of silently sanitizing.
    // Use these when validating content received from the network, which
    // should already be clean (sent through sanitize_for_storage by peers).
    // ========================================================================

    /// Returns [`ValidationError::ContainsAnsiEscapes`] if the text
    /// contains any ESC byte (`\x1b`).
    pub fn check_for_ansi_escapes(text: &str) -> ValidationResult<()> {
        if text.contains('\x1b') {
            Err(ValidationError::ContainsAnsiEscapes)
        } else {
            Ok(())
        }
    }

    /// Returns [`ValidationError::ContainsControlCharacters`] if the text
    /// contains any control character other than those allowed by
    /// [`Self::is_allowed_whitespace`].
    pub fn check_for_control_characters(text: &str) -> ValidationResult<()> {
        if text
            .chars()
            .any(|c| c.is_control() && !Self::is_allowed_whitespace(c))
        {
            Err(ValidationError::ContainsControlCharacters)
        } else {
            Ok(())
        }
    }

    /// Returns [`ValidationError::ContainsBinaryData`] if the text contains
    /// a null byte (`\0`).
    pub fn check_for_binary_data(text: &str) -> ValidationResult<()> {
        if text.contains('\0') {
            Err(ValidationError::ContainsBinaryData)
        } else {
            Ok(())
        }
    }
}

pub struct ContentValidator;

impl ContentValidator {
    // ========================================================================
    // Private pipeline helper
    // ========================================================================

    /// Common validation pipeline: sanitize → empty check → length check →
    /// optional character check → return.
    ///
    /// * `max_chars` — limit measured in Unicode scalar values (characters),
    ///   consistent with the "characters" wording in [`ValidationError::TooLong`].
    /// * `char_predicate` — when `Some`, every character in the output must
    ///   satisfy the predicate; failing characters are collected and returned
    ///   as [`ValidationError::InvalidCharacters`].
    /// * `trim_output` — when `true`, leading/trailing whitespace is stripped
    ///   from the sanitized text before all checks and the trimmed value is
    ///   returned; when `false`, the raw sanitized text is used and returned.
    fn validate_text(
        input: &str,
        max_chars: usize,
        char_predicate: Option<fn(char) -> bool>,
        trim_output: bool,
    ) -> ValidationResult<String> {
        let sanitized = ContentSanitizer::sanitize_for_storage(input);

        // Empty check always uses the trimmed form so that whitespace-only
        // strings are rejected regardless of `trim_output`.
        if sanitized.trim().is_empty() {
            return Err(ValidationError::Empty);
        }

        let output = if trim_output {
            sanitized.trim().to_string()
        } else {
            sanitized
        };

        let char_count = output.chars().count();
        if char_count > max_chars {
            return Err(ValidationError::TooLong {
                max_length: max_chars,
                actual_length: char_count,
            });
        }

        if let Some(is_valid) = char_predicate {
            let invalid: Vec<char> = output.chars().filter(|&ch| !is_valid(ch)).collect();
            if !invalid.is_empty() {
                return Err(ValidationError::InvalidCharacters {
                    invalid_chars: invalid,
                });
            }
        }

        Ok(output)
    }

    /// Characters allowed in identifier-style names: channel names, peer
    /// names, and WASM offering names.  Replaces the three identical
    /// predicates that existed before this refactor.
    fn is_valid_identifier_char(ch: char) -> bool {
        ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.')
    }

    // ========================================================================
    // Story validators
    // ========================================================================

    pub fn validate_story_name(name: &str) -> ValidationResult<String> {
        // `sanitize_for_storage` already removes the characters that a
        // name-char predicate would reject, so no extra char check is needed.
        Self::validate_text(name, ContentLimits::STORY_NAME_MAX, None, true)
    }

    pub fn validate_story_header(header: &str) -> ValidationResult<String> {
        Self::validate_text(header, ContentLimits::STORY_HEADER_MAX, None, true)
    }

    pub fn validate_story_body(body: &str) -> ValidationResult<String> {
        // Body is returned untrimmed so that intentional leading/trailing
        // whitespace chosen by the author is preserved.
        Self::validate_text(body, ContentLimits::STORY_BODY_MAX, None, false)
    }

    // ========================================================================
    // Channel validators
    // ========================================================================

    pub fn validate_channel_name(name: &str) -> ValidationResult<String> {
        Self::validate_text(
            name,
            ContentLimits::CHANNEL_NAME_MAX,
            Some(Self::is_valid_identifier_char),
            true,
        )
    }

    pub fn validate_channel_description(description: &str) -> ValidationResult<String> {
        Self::validate_text(
            description,
            ContentLimits::CHANNEL_DESCRIPTION_MAX,
            None,
            true,
        )
    }

    // ========================================================================
    // Peer / node validators
    // ========================================================================

    pub fn validate_peer_name(name: &str) -> ValidationResult<String> {
        Self::validate_text(
            name,
            ContentLimits::PEER_NAME_MAX,
            Some(Self::is_valid_identifier_char),
            true,
        )
    }

    pub fn validate_node_description(description: &str) -> ValidationResult<String> {
        Self::validate_text(
            description,
            ContentLimits::NODE_DESCRIPTION_MAX,
            None,
            false,
        )
    }

    pub fn validate_direct_message(message: &str) -> ValidationResult<String> {
        Self::validate_text(message, ContentLimits::DIRECT_MESSAGE_MAX, None, false)
    }

    pub fn validate_story_id(id_str: &str) -> ValidationResult<usize> {
        id_str
            .trim()
            .parse::<usize>()
            .map_err(|_| ValidationError::InvalidFormat {
                expected: "positive integer".to_string(),
            })
    }

    // ========================================================================
    // WASM Offering Validation
    // ========================================================================

    /// Validate a WASM offering name
    pub fn validate_wasm_offering_name(name: &str) -> ValidationResult<String> {
        Self::validate_text(
            name,
            ContentLimits::WASM_OFFERING_NAME_MAX,
            Some(Self::is_valid_identifier_char),
            true,
        )
    }

    /// Validate a WASM offering description
    pub fn validate_wasm_offering_description(description: &str) -> ValidationResult<String> {
        Self::validate_text(
            description,
            ContentLimits::WASM_OFFERING_DESCRIPTION_MAX,
            None,
            true,
        )
    }

    /// Validate an IPFS CID
    pub fn validate_ipfs_cid(cid: &str) -> ValidationResult<String> {
        let sanitized = Self::validate_text(cid, ContentLimits::WASM_IPFS_CID_MAX, None, true)?;

        // IPFS CID v0 starts with "Qm", CID v1 typically starts with "bafy"
        if !sanitized.starts_with("Qm") && !sanitized.starts_with("bafy") {
            return Err(ValidationError::InvalidFormat {
                expected: "Valid IPFS CID (starting with Qm or bafy)".to_string(),
            });
        }

        // CID v0 is base58btc encoded and typically 46 characters - which is not enforced here
        // CID v1 can vary in length but should be alphanumeric
        if !sanitized.chars().all(|c| c.is_alphanumeric()) {
            return Err(ValidationError::InvalidFormat {
                expected: "Alphanumeric IPFS CID".to_string(),
            });
        }

        Ok(sanitized)
    }

    /// Validate a semantic version string (X.Y.Z format)
    pub fn validate_wasm_version(version: &str) -> ValidationResult<String> {
        let sanitized = Self::validate_text(version, ContentLimits::WASM_VERSION_MAX, None, true)?;

        // Basic semantic version validation: should have numeric parts separated by dots
        let parts: Vec<&str> = sanitized.split('.').collect();
        if parts.len() < 2 || parts.len() > 4 {
            return Err(ValidationError::InvalidFormat {
                expected: "Semantic version (e.g., 1.0.0)".to_string(),
            });
        }

        for part in &parts {
            // Allow optional pre-release suffix like -alpha, -beta, -rc1
            let numeric_part = part.split('-').next().unwrap_or(*part);
            if !numeric_part.chars().all(|c| c.is_ascii_digit()) {
                return Err(ValidationError::InvalidFormat {
                    expected: "Semantic version with numeric components".to_string(),
                });
            }
        }

        Ok(sanitized)
    }

    /// Validate a WASM parameter name
    pub fn validate_wasm_param_name(name: &str) -> ValidationResult<String> {
        let sanitized = Self::validate_text(
            name,
            ContentLimits::WASM_PARAM_NAME_MAX,
            Some(|ch: char| ch.is_alphanumeric() || ch == '_'),
            true,
        )?;

        // First character should not be a digit
        if sanitized
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            return Err(ValidationError::InvalidFormat {
                expected: "Parameter name starting with letter or underscore".to_string(),
            });
        }

        Ok(sanitized)
    }

    /// Validate a WASM parameter type
    pub fn validate_wasm_param_type(param_type: &str) -> ValidationResult<String> {
        let sanitized =
            Self::validate_text(param_type, ContentLimits::WASM_PARAM_TYPE_MAX, None, true)?;

        let type_lower = sanitized.to_lowercase();
        if !WASM_PARAM_TYPES.contains(&type_lower.as_str()) {
            return Err(ValidationError::InvalidFormat {
                expected: format!(
                    "Valid parameter type (one of: {})",
                    WASM_PARAM_TYPES.join(", ")
                ),
            });
        }

        Ok(type_lower)
    }

    // ========================================================================
    // Strict network-content validation
    // ========================================================================

    /// Strictly validate text received from a network peer.
    ///
    /// Unlike the storage-oriented validators, this does **not** sanitize —
    /// it rejects on the first problem found:
    ///
    /// * [`ValidationError::ContainsAnsiEscapes`] — ESC byte present
    /// * [`ValidationError::ContainsControlCharacters`] — non-whitespace
    ///   control character present
    /// * [`ValidationError::ContainsBinaryData`] — null byte present
    ///
    /// Use this to reject peer content that arrives already dirty, which may
    /// indicate a misbehaving or malicious peer.
    pub fn validate_received_content(text: &str) -> ValidationResult<()> {
        ContentSanitizer::check_for_ansi_escapes(text)?;
        ContentSanitizer::check_for_control_characters(text)?;
        ContentSanitizer::check_for_binary_data(text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Sanitizer tests
    // ========================================================================
    mod sanitizer {
        use super::*;

        #[test]
        fn test_content_limits() {
            assert_eq!(ContentLimits::STORY_NAME_MAX, 100);
            assert_eq!(ContentLimits::STORY_HEADER_MAX, 200);
            assert_eq!(ContentLimits::STORY_BODY_MAX, 10_000);
            assert_eq!(ContentLimits::CHANNEL_NAME_MAX, 50);
            assert_eq!(ContentLimits::CHANNEL_DESCRIPTION_MAX, 200);
            assert_eq!(ContentLimits::PEER_NAME_MAX, 30);
            assert_eq!(ContentLimits::DIRECT_MESSAGE_MAX, 1_000);
        }

        #[test]
        fn test_ansi_escape_removal() {
            let text_with_ansi = "Hello \x1b[31mRed Text\x1b[0m World";
            let cleaned = ContentSanitizer::strip_ansi_escapes(text_with_ansi);
            assert_eq!(cleaned, "Hello Red Text World");
        }

        #[test]
        fn test_ansi_escape_edge_cases() {
            // Test CSI sequences
            assert_eq!(
                ContentSanitizer::strip_ansi_escapes("Before\x1b[31mColor\x1b[0mAfter"),
                "BeforeColorAfter"
            );

            // Test OSC sequences with BEL terminator
            assert_eq!(
                ContentSanitizer::strip_ansi_escapes("Test\x1b]0;Title\x07End"),
                "TestEnd"
            );

            // Test OSC sequences with ESC \ terminator
            assert_eq!(
                ContentSanitizer::strip_ansi_escapes("Test\x1b]0;Title\x1b\\End"),
                "TestEnd"
            );

            // Test charset sequences
            assert_eq!(
                ContentSanitizer::strip_ansi_escapes("Test\x1b(BCharset\x1b)0End"),
                "TestCharsetEnd"
            );

            // Test single character sequences
            assert_eq!(
                ContentSanitizer::strip_ansi_escapes("Test\x1bMReverse"),
                "TestReverse"
            );

            // Test complex mixed sequences
            let complex = "Start\x1b[1;31mBold Red\x1b[0m\x1b]0;Window Title\x07\x1b(BNormal";
            assert_eq!(
                ContentSanitizer::strip_ansi_escapes(complex),
                "StartBold RedNormal"
            );
        }

        #[test]
        fn test_control_character_removal() {
            let text_with_controls = "Hello\x00\x01\x02 World\t\n";
            let cleaned = ContentSanitizer::strip_control_characters(text_with_controls);
            assert_eq!(cleaned, "Hello World\t\n");
        }

        #[test]
        fn test_binary_data_filtering_edge_cases() {
            // Test null byte filtering
            assert_eq!(
                ContentSanitizer::strip_binary_data("Hello\x00World"),
                "HelloWorld"
            );

            // Test that whitespace is preserved
            assert_eq!(
                ContentSanitizer::strip_binary_data("Hello\t\n\r World"),
                "Hello\t\n\r World"
            );

            // Test control character removal but whitespace preservation
            assert_eq!(
                ContentSanitizer::strip_binary_data("Hello\x01\x02\x03 World\t\n"),
                "Hello World\t\n"
            );

            // Test mixed binary and control characters
            assert_eq!(
                ContentSanitizer::strip_binary_data("Test\x00\x01 \x02Content\t"),
                "Test Content\t"
            );
        }

        #[test]
        fn test_sanitize_for_display_single_pass() {
            // Verify that sanitize_for_display produces the same result as the
            // old two-step approach (strip_control_characters + strip_binary_data).
            let input = "Hello \x1b[31mWorld\x1b[0m\x00\x01 Test";
            let display = ContentSanitizer::sanitize_for_display(input);
            assert_eq!(display, "Hello World Test");
            assert!(!display.contains('\x1b'));
            assert!(!display.contains('\0'));
        }

        #[test]
        fn test_terminal_injection_payloads() {
            let injection_patterns = vec![
                "\x1b[2J\x1b[H",              // Clear screen and move cursor
                "\x1b]0;Malicious Title\x07", // Set window title
                "\x1b[?25l",                  // Hide cursor
                "\x1b[999;999H",              // Move cursor far
                "\x1b[0c",                    // Device attributes request
            ];

            for pattern in injection_patterns {
                let test_content = format!("Normal text {} more text", pattern);
                let sanitized = ContentSanitizer::sanitize_for_display(&test_content);
                assert!(!sanitized.contains('\x1b'));
                assert_eq!(sanitized, "Normal text  more text");
            }

            // Control character injection
            let control_chars = "Test\x00\x01\x02\x03\x04\x05Content";
            let sanitized = ContentSanitizer::sanitize_for_display(control_chars);
            assert_eq!(sanitized, "TestContent");

            // Mixed injection attempts
            let complex_injection = "Start\x1b[31m\x00Evil\x01Content\x1b[0m\x02End";
            let sanitized = ContentSanitizer::sanitize_for_display(complex_injection);
            assert_eq!(sanitized, "StartEvilContentEnd");
        }

        #[test]
        fn test_comprehensive_sanitization() {
            let malicious_input = "Hello \x1b[31mWorld\x1b[0m\x00\x01 Test";
            let sanitized = ContentSanitizer::sanitize_for_display(malicious_input);
            assert_eq!(sanitized, "Hello World Test");
        }

        #[test]
        fn test_maximum_length_performance() {
            // Test with maximum story body length
            let max_content = "a".repeat(ContentLimits::STORY_BODY_MAX);
            let result = ContentValidator::validate_story_body(&max_content);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().len(), ContentLimits::STORY_BODY_MAX);

            // Test with over-limit content
            let over_limit = "a".repeat(ContentLimits::STORY_BODY_MAX + 1);
            assert!(ContentValidator::validate_story_body(&over_limit).is_err());

            // Test sanitization performance with large content
            let large_content_with_ansi = format!("\x1b[31m{}\x1b[0m", "a".repeat(5000));
            let sanitized = ContentSanitizer::sanitize_for_display(&large_content_with_ansi);
            assert_eq!(sanitized.len(), 5000);
            assert!(!sanitized.contains('\x1b'));
        }
    }

    // ========================================================================
    // General validator tests
    // ========================================================================
    mod general_validation {
        use super::*;

        #[test]
        fn test_story_name_validation() {
            // Valid name
            assert!(ContentValidator::validate_story_name("My Story").is_ok());

            // Empty name
            assert!(ContentValidator::validate_story_name("").is_err());

            // Too long name
            let long_name = "a".repeat(101);
            assert!(ContentValidator::validate_story_name(&long_name).is_err());

            // Binary data should be filtered out during sanitization, not cause error
            let result = ContentValidator::validate_story_name("Story\x00");
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "Story");
        }

        #[test]
        fn test_channel_name_validation() {
            // Valid channel name
            assert!(ContentValidator::validate_channel_name("general").is_ok());
            assert!(ContentValidator::validate_channel_name("my-channel_v2").is_ok());

            // Invalid characters
            assert!(ContentValidator::validate_channel_name("my channel").is_err());
            assert!(ContentValidator::validate_channel_name("channel!").is_err());
        }

        #[test]
        fn test_channel_description_uses_own_limit() {
            // CHANNEL_DESCRIPTION_MAX should be used, not STORY_HEADER_MAX
            let at_limit = "a".repeat(ContentLimits::CHANNEL_DESCRIPTION_MAX);
            assert!(ContentValidator::validate_channel_description(&at_limit).is_ok());

            let over_limit = "a".repeat(ContentLimits::CHANNEL_DESCRIPTION_MAX + 1);
            assert!(ContentValidator::validate_channel_description(&over_limit).is_err());
        }

        #[test]
        fn test_peer_name_validation() {
            // Valid peer names
            assert!(ContentValidator::validate_peer_name("alice").is_ok());
            assert!(ContentValidator::validate_peer_name("user_123").is_ok());

            // Invalid characters
            assert!(ContentValidator::validate_peer_name("alice smith").is_err());
            assert!(ContentValidator::validate_peer_name("user@domain").is_err());
        }

        #[test]
        fn test_unicode_edge_cases() {
            // Test emoji handling
            let emoji_text = "Hello 👋 World 🌍 Test 🚀";
            assert!(ContentValidator::validate_story_name(emoji_text).is_ok());

            // Test RTL text (Arabic)
            let rtl_text = "Hello مرحبا World";
            let result = ContentValidator::validate_story_name(rtl_text);
            assert!(result.is_ok());

            // Test mixed scripts
            let mixed_text = "English 中文 Español";
            assert!(ContentValidator::validate_story_name(mixed_text).is_ok());

            // Test zero-width characters (should be filtered out)
            let zero_width = "Hello\u{200B}World"; // Zero-width space
            let result = ContentValidator::validate_story_name(zero_width);
            assert!(result.is_ok());
        }

        #[test]
        fn test_malformed_utf8_handling() {
            // Test with valid UTF-8
            let valid_utf8 = "Hello 世界";
            assert!(ContentValidator::validate_story_name(valid_utf8).is_ok());

            // Test with combining characters
            let combining = "e\u{0301}"; // e with acute accent
            assert!(ContentValidator::validate_story_name(combining).is_ok());

            // Test with surrogate-like patterns (valid in UTF-8)
            let special_chars = "Test\u{FEFF}Content"; // BOM character
            let result = ContentValidator::validate_story_name(special_chars);
            assert!(result.is_ok());
        }

        #[test]
        fn test_improved_error_messages() {
            // Test channel name with invalid characters shows helpful error
            let result = ContentValidator::validate_channel_name("my channel!");
            assert!(result.is_err());
            let error_msg = result.unwrap_err().to_string();
            assert!(error_msg.contains("Valid characters: alphanumeric"));

            // Test peer name with invalid characters
            let result = ContentValidator::validate_peer_name("user@domain.com");
            assert!(result.is_err());
            let error_msg = result.unwrap_err().to_string();
            assert!(error_msg.contains("Valid characters: alphanumeric"));
        }

        #[test]
        fn test_length_measured_in_chars_not_bytes() {
            // A 100-char string of 3-byte UTF-8 characters must pass (100 chars ≤ 100 limit)
            // whereas it would have failed when measuring bytes (300 > 100).
            let japanese = "あ".repeat(ContentLimits::STORY_NAME_MAX);
            assert_eq!(japanese.chars().count(), ContentLimits::STORY_NAME_MAX);
            let result = ContentValidator::validate_story_name(&japanese);
            assert!(
                result.is_ok(),
                "100 Japanese chars should be within the 100-char limit"
            );

            let too_long = "あ".repeat(ContentLimits::STORY_NAME_MAX + 1);
            let result = ContentValidator::validate_story_name(&too_long);
            assert!(
                result.is_err(),
                "101 Japanese chars should exceed the 100-char limit"
            );
            if let Err(ValidationError::TooLong { actual_length, .. }) = result {
                assert_eq!(actual_length, ContentLimits::STORY_NAME_MAX + 1);
            }
        }
    }

    // ========================================================================
    // WASM validator tests
    // ========================================================================
    mod wasm_validation {
        use super::*;

        #[test]
        fn test_wasm_param_types_constant() {
            // Ensure the module-level constant is used and contains expected types
            assert!(WASM_PARAM_TYPES.contains(&"string"));
            assert!(WASM_PARAM_TYPES.contains(&"int"));
            assert!(!WASM_PARAM_TYPES.contains(&"unknown"));
        }

        #[test]
        fn test_validate_wasm_param_type_valid() {
            for &t in WASM_PARAM_TYPES {
                assert!(
                    ContentValidator::validate_wasm_param_type(t).is_ok(),
                    "type '{t}' should be valid"
                );
                // Case-insensitive
                assert!(
                    ContentValidator::validate_wasm_param_type(&t.to_uppercase()).is_ok(),
                    "uppercase type '{t}' should be valid"
                );
            }
        }

        #[test]
        fn test_validate_wasm_param_type_invalid() {
            let result = ContentValidator::validate_wasm_param_type("unknown");
            assert!(result.is_err());
            let msg = result.unwrap_err().to_string();
            assert!(msg.contains("string"));
        }

        #[test]
        fn test_validate_wasm_param_name() {
            assert!(ContentValidator::validate_wasm_param_name("my_param").is_ok());
            assert!(ContentValidator::validate_wasm_param_name("_private").is_ok());

            // Must not start with digit
            assert!(ContentValidator::validate_wasm_param_name("1bad").is_err());

            // No hyphens or dots (stricter than identifier_char)
            assert!(ContentValidator::validate_wasm_param_name("bad-name").is_err());

            // Empty
            assert!(ContentValidator::validate_wasm_param_name("").is_err());
        }

        #[test]
        fn test_validate_wasm_offering_name() {
            assert!(ContentValidator::validate_wasm_offering_name("my-module_v1.0").is_ok());
            assert!(ContentValidator::validate_wasm_offering_name("bad name!").is_err());
        }

        #[test]
        fn test_validate_wasm_version() {
            assert!(ContentValidator::validate_wasm_version("1.0.0").is_ok());
            assert!(ContentValidator::validate_wasm_version("2.1").is_ok());
            assert!(ContentValidator::validate_wasm_version("1.0.0-alpha").is_ok());

            // Too few / too many parts
            assert!(ContentValidator::validate_wasm_version("1").is_err());
            assert!(ContentValidator::validate_wasm_version("1.2.3.4.5").is_err());

            // Non-numeric component
            assert!(ContentValidator::validate_wasm_version("1.x.0").is_err());
        }

        #[test]
        fn test_validate_ipfs_cid() {
            // Valid CID v0 prefix
            let fake_v0 = format!("Qm{}", "a".repeat(44));
            assert!(ContentValidator::validate_ipfs_cid(&fake_v0).is_ok());

            // Valid CID v1 prefix
            let fake_v1 = format!("bafy{}", "a".repeat(44));
            assert!(ContentValidator::validate_ipfs_cid(&fake_v1).is_ok());

            // Wrong prefix
            assert!(ContentValidator::validate_ipfs_cid("abc123").is_err());

            // Non-alphanumeric
            assert!(ContentValidator::validate_ipfs_cid("Qm-dashes-not-allowed").is_err());

            // Empty
            assert!(ContentValidator::validate_ipfs_cid("").is_err());
        }
    }

    // ========================================================================
    // Strict network-content validation tests
    // ========================================================================
    mod strict_validation {
        use super::*;

        #[test]
        fn test_check_for_ansi_escapes_clean() {
            assert!(ContentSanitizer::check_for_ansi_escapes("Hello World").is_ok());
            assert!(ContentSanitizer::check_for_ansi_escapes("Line1\nLine2").is_ok());
        }

        #[test]
        fn test_check_for_ansi_escapes_dirty() {
            let result = ContentSanitizer::check_for_ansi_escapes("Hello \x1b[31mRed\x1b[0m");
            assert_eq!(result, Err(ValidationError::ContainsAnsiEscapes));
        }

        #[test]
        fn test_check_for_control_characters_clean() {
            assert!(ContentSanitizer::check_for_control_characters("Hello\tWorld\n").is_ok());
        }

        #[test]
        fn test_check_for_control_characters_dirty() {
            let result = ContentSanitizer::check_for_control_characters("Hello\x01World");
            assert_eq!(result, Err(ValidationError::ContainsControlCharacters));
        }

        #[test]
        fn test_check_for_binary_data_clean() {
            assert!(ContentSanitizer::check_for_binary_data("Hello World").is_ok());
        }

        #[test]
        fn test_check_for_binary_data_dirty() {
            let result = ContentSanitizer::check_for_binary_data("Hello\x00World");
            assert_eq!(result, Err(ValidationError::ContainsBinaryData));
        }

        #[test]
        fn test_validate_received_content_clean() {
            assert!(ContentValidator::validate_received_content("Hello, peer!").is_ok());
            assert!(ContentValidator::validate_received_content("Line1\nLine2\t").is_ok());
        }

        #[test]
        fn test_validate_received_content_rejects_ansi() {
            let result =
                ContentValidator::validate_received_content("Good text \x1b[31mbad\x1b[0m");
            assert_eq!(result, Err(ValidationError::ContainsAnsiEscapes));
        }

        #[test]
        fn test_validate_received_content_rejects_control_chars() {
            let result = ContentValidator::validate_received_content("text\x02with\x03controls");
            assert_eq!(result, Err(ValidationError::ContainsControlCharacters));
        }

        #[test]
        fn test_validate_received_content_rejects_binary_data() {
            // \x00 is a control character, so check_for_control_characters catches
            // it before check_for_binary_data is reached.
            let result = ContentValidator::validate_received_content("text\x00with\x00nulls");
            assert_eq!(result, Err(ValidationError::ContainsControlCharacters));
        }

        #[test]
        fn test_check_for_binary_data_alone_rejects_null() {
            // Directly test check_for_binary_data to confirm it returns
            // ContainsBinaryData (bypassing the control-char check).
            let result = ContentSanitizer::check_for_binary_data("text\x00nulls");
            assert_eq!(result, Err(ValidationError::ContainsBinaryData));
        }

        #[test]
        fn test_error_display_for_strict_variants() {
            assert!(
                ValidationError::ContainsAnsiEscapes
                    .to_string()
                    .contains("ANSI")
            );
            assert!(
                ValidationError::ContainsControlCharacters
                    .to_string()
                    .contains("control")
            );
            assert!(
                ValidationError::ContainsBinaryData
                    .to_string()
                    .contains("binary")
            );
        }
    }
}
