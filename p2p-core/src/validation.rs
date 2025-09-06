//! Content validation and sanitization module for P2P-Play
//!
//! This module provides comprehensive input validation and sanitization functions
//! to prevent security vulnerabilities including:
//! - ANSI escape sequence injection
//! - Terminal control character injection  
//! - Content length abuse/resource exhaustion
//! - Invalid character injection
//! - Binary data injection

/// Maximum content lengths for different input types
pub struct ContentLimits;

impl ContentLimits {
    pub const STORY_NAME_MAX: usize = 100;
    pub const STORY_HEADER_MAX: usize = 200;
    pub const STORY_BODY_MAX: usize = 10_000;
    pub const CHANNEL_NAME_MAX: usize = 50;
    pub const PEER_NAME_MAX: usize = 30;
    pub const DIRECT_MESSAGE_MAX: usize = 1_000;
    pub const NODE_DESCRIPTION_MAX: usize = 2_000;
}

/// Validation error types
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

/// Content sanitization functions
pub struct ContentSanitizer;

impl ContentSanitizer {
    /// Remove ANSI escape sequences from text
    pub fn strip_ansi_escapes(text: &str) -> String {
        let mut result = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                // Handle different types of ANSI escape sequences
                if let Some(&next_ch) = chars.peek() {
                    match next_ch {
                        '[' => {
                            // CSI sequences: ESC [ ... final_char
                            chars.next(); // consume '['
                            #[allow(clippy::while_let_on_iterator)]
                            while let Some(seq_ch) = chars.next() {
                                if seq_ch.is_ascii_alphabetic() || seq_ch == '~' {
                                    break;
                                }
                            }
                        }
                        ']' => {
                            // OSC sequences: ESC ] ... ST (either BEL \x07 or ESC \)
                            chars.next(); // consume ']'
                            #[allow(clippy::while_let_on_iterator)]
                            while let Some(seq_ch) = chars.next() {
                                if seq_ch == '\x07' {
                                    // BEL terminator
                                    break;
                                } else if seq_ch == '\x1b' && chars.peek() == Some(&'\\') {
                                    // ESC \ terminator
                                    chars.next(); // consume '\'
                                    break;
                                }
                            }
                        }
                        '(' | ')' | '*' | '+' => {
                            // Charset sequences: ESC ( char, ESC ) char, etc.
                            chars.next(); // consume intermediate
                            chars.next(); // consume final char
                        }
                        _ => {
                            // Single character sequences or others: ESC char
                            if next_ch.is_ascii_alphabetic() {
                                chars.next(); // consume the character
                            }
                        }
                    }
                    continue;
                }
            }
            result.push(ch);
        }

        result
    }

    /// Remove terminal control characters (but preserve common whitespace)
    pub fn strip_control_characters(text: &str) -> String {
        text.chars()
            .filter(|&ch| {
                // Allow common whitespace characters
                if ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' {
                    return true;
                }
                // Remove other control characters
                !ch.is_control()
            })
            .collect()
    }

    /// Remove null bytes and other binary data
    pub fn strip_binary_data(text: &str) -> String {
        text.chars()
            .filter(|&ch| {
                ch != '\0' && (!ch.is_control() || matches!(ch, ' ' | '\t' | '\n' | '\r'))
            })
            .collect()
    }

    /// Comprehensive sanitization for display content
    pub fn sanitize_for_display(text: &str) -> String {
        let text = Self::strip_ansi_escapes(text);
        let text = Self::strip_control_characters(&text);
        Self::strip_binary_data(&text)
    }

    /// Sanitization for storage (more permissive, allows newlines)
    pub fn sanitize_for_storage(text: &str) -> String {
        let text = Self::strip_ansi_escapes(text);
        // For storage, we allow newlines and tabs but remove other control chars
        text.chars()
            .filter(|&ch| ch != '\0' && (!ch.is_control() || matches!(ch, '\n' | '\r' | '\t')))
            .collect()
    }
}

/// Content validation functions
pub struct ContentValidator;

impl ContentValidator {
    /// Validate story name
    pub fn validate_story_name(name: &str) -> ValidationResult<String> {
        let sanitized = ContentSanitizer::sanitize_for_storage(name);

        if sanitized.trim().is_empty() {
            return Err(ValidationError::Empty);
        }

        if sanitized.len() > ContentLimits::STORY_NAME_MAX {
            return Err(ValidationError::TooLong {
                max_length: ContentLimits::STORY_NAME_MAX,
                actual_length: sanitized.len(),
            });
        }

        // Check for reasonable character set (alphanumeric, spaces, basic punctuation)
        let invalid_chars: Vec<char> = sanitized
            .chars()
            .filter(|&ch| !Self::is_valid_name_char(ch))
            .collect();

        if !invalid_chars.is_empty() {
            return Err(ValidationError::InvalidCharacters { invalid_chars });
        }

        Ok(sanitized.trim().to_string())
    }

    /// Validate story header  
    pub fn validate_story_header(header: &str) -> ValidationResult<String> {
        let sanitized = ContentSanitizer::sanitize_for_storage(header);

        if sanitized.trim().is_empty() {
            return Err(ValidationError::Empty);
        }

        if sanitized.len() > ContentLimits::STORY_HEADER_MAX {
            return Err(ValidationError::TooLong {
                max_length: ContentLimits::STORY_HEADER_MAX,
                actual_length: sanitized.len(),
            });
        }

        Ok(sanitized.trim().to_string())
    }

    /// Validate story body
    pub fn validate_story_body(body: &str) -> ValidationResult<String> {
        let sanitized = ContentSanitizer::sanitize_for_storage(body);

        if sanitized.trim().is_empty() {
            return Err(ValidationError::Empty);
        }

        if sanitized.len() > ContentLimits::STORY_BODY_MAX {
            return Err(ValidationError::TooLong {
                max_length: ContentLimits::STORY_BODY_MAX,
                actual_length: sanitized.len(),
            });
        }

        Ok(sanitized)
    }

    /// Validate channel name
    pub fn validate_channel_name(name: &str) -> ValidationResult<String> {
        let sanitized = ContentSanitizer::sanitize_for_storage(name);

        if sanitized.trim().is_empty() {
            return Err(ValidationError::Empty);
        }

        if sanitized.len() > ContentLimits::CHANNEL_NAME_MAX {
            return Err(ValidationError::TooLong {
                max_length: ContentLimits::CHANNEL_NAME_MAX,
                actual_length: sanitized.len(),
            });
        }

        // Channel names should be more restrictive
        let invalid_chars: Vec<char> = sanitized
            .chars()
            .filter(|&ch| !Self::is_valid_channel_name_char(ch))
            .collect();

        if !invalid_chars.is_empty() {
            return Err(ValidationError::InvalidCharacters { invalid_chars });
        }

        Ok(sanitized.trim().to_string())
    }

    /// Validate channel description
    pub fn validate_channel_description(description: &str) -> ValidationResult<String> {
        let sanitized = ContentSanitizer::sanitize_for_storage(description);

        if sanitized.trim().is_empty() {
            return Err(ValidationError::Empty);
        }

        if sanitized.len() > ContentLimits::STORY_HEADER_MAX {
            return Err(ValidationError::TooLong {
                max_length: ContentLimits::STORY_HEADER_MAX,
                actual_length: sanitized.len(),
            });
        }

        Ok(sanitized.trim().to_string())
    }

    /// Validate peer name
    pub fn validate_peer_name(name: &str) -> ValidationResult<String> {
        let sanitized = ContentSanitizer::sanitize_for_storage(name);

        if sanitized.trim().is_empty() {
            return Err(ValidationError::Empty);
        }

        if sanitized.len() > ContentLimits::PEER_NAME_MAX {
            return Err(ValidationError::TooLong {
                max_length: ContentLimits::PEER_NAME_MAX,
                actual_length: sanitized.len(),
            });
        }

        // Peer names should be restrictive for network safety
        let invalid_chars: Vec<char> = sanitized
            .chars()
            .filter(|&ch| !Self::is_valid_peer_name_char(ch))
            .collect();

        if !invalid_chars.is_empty() {
            return Err(ValidationError::InvalidCharacters { invalid_chars });
        }

        Ok(sanitized.trim().to_string())
    }

    /// Validate direct message content
    pub fn validate_direct_message(message: &str) -> ValidationResult<String> {
        let sanitized = ContentSanitizer::sanitize_for_storage(message);

        if sanitized.trim().is_empty() {
            return Err(ValidationError::Empty);
        }

        if sanitized.len() > ContentLimits::DIRECT_MESSAGE_MAX {
            return Err(ValidationError::TooLong {
                max_length: ContentLimits::DIRECT_MESSAGE_MAX,
                actual_length: sanitized.len(),
            });
        }

        Ok(sanitized)
    }

    /// Validate node description
    pub fn validate_node_description(description: &str) -> ValidationResult<String> {
        let sanitized = ContentSanitizer::sanitize_for_storage(description);

        if sanitized.trim().is_empty() {
            return Err(ValidationError::Empty);
        }

        if sanitized.len() > ContentLimits::NODE_DESCRIPTION_MAX {
            return Err(ValidationError::TooLong {
                max_length: ContentLimits::NODE_DESCRIPTION_MAX,
                actual_length: sanitized.len(),
            });
        }

        Ok(sanitized)
    }

    /// Check if character is valid for general names (stories, etc.)
    fn is_valid_name_char(ch: char) -> bool {
        // Allow most Unicode characters for story names, but reject problematic ones
        if ch.is_control() && !matches!(ch, '\n' | '\r' | '\t') {
            return false;
        }

        // Reject null bytes and other dangerous characters
        if ch == '\0' || ch == '\x1b' {
            return false;
        }

        // Allow all other Unicode characters including emojis, accented characters, etc.
        true
    }

    /// Check if character is valid for channel names (more restrictive)
    fn is_valid_channel_name_char(ch: char) -> bool {
        ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.')
    }

    /// Check if character is valid for peer names (most restrictive)
    fn is_valid_peer_name_char(ch: char) -> bool {
        ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.')
    }

    /// Validate numeric ID format
    pub fn validate_story_id(id_str: &str) -> ValidationResult<usize> {
        match id_str.trim().parse::<usize>() {
            Ok(id) => Ok(id),
            Err(_) => Err(ValidationError::InvalidFormat {
                expected: "positive integer".to_string(),
            }),
        }
    }

    /// Validate and sanitize file paths to prevent directory traversal
    pub fn validate_safe_filename(filename: &str) -> ValidationResult<String> {
        let sanitized = ContentSanitizer::sanitize_for_storage(filename);

        if sanitized.is_empty() {
            return Err(ValidationError::Empty);
        }

        // Check for directory traversal attempts
        if sanitized.contains("..") || sanitized.contains('/') || sanitized.contains('\\') {
            return Err(ValidationError::InvalidFormat {
                expected: "filename without path separators".to_string(),
            });
        }

        // Check for reserved characters
        let invalid_chars: Vec<char> = sanitized
            .chars()
            .filter(|&ch| matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\0'))
            .collect();

        if !invalid_chars.is_empty() {
            return Err(ValidationError::InvalidCharacters { invalid_chars });
        }

        Ok(sanitized)
    }
}

/// Security utilities
pub struct SecurityValidator;

impl SecurityValidator {
    /// Check if text contains ANSI escape sequences
    pub fn contains_ansi_escapes(text: &str) -> bool {
        text.contains('\x1b')
    }

    /// Check if text contains control characters (excluding common whitespace)
    pub fn contains_control_characters(text: &str) -> bool {
        text.chars()
            .any(|ch| ch.is_control() && !matches!(ch, ' ' | '\t' | '\n' | '\r'))
    }

    /// Check if text contains null bytes or other binary data
    pub fn contains_binary_data(text: &str) -> bool {
        text.chars().any(|ch| ch == '\0')
    }

    /// Comprehensive security check
    pub fn is_safe_content(text: &str) -> Result<(), ValidationError> {
        if Self::contains_ansi_escapes(text) {
            return Err(ValidationError::ContainsAnsiEscapes);
        }

        if Self::contains_control_characters(text) {
            return Err(ValidationError::ContainsControlCharacters);
        }

        if Self::contains_binary_data(text) {
            return Err(ValidationError::ContainsBinaryData);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_limits() {
        assert_eq!(ContentLimits::STORY_NAME_MAX, 100);
        assert_eq!(ContentLimits::STORY_HEADER_MAX, 200);
        assert_eq!(ContentLimits::STORY_BODY_MAX, 10_000);
        assert_eq!(ContentLimits::CHANNEL_NAME_MAX, 50);
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
    fn test_peer_name_validation() {
        // Valid peer names
        assert!(ContentValidator::validate_peer_name("alice").is_ok());
        assert!(ContentValidator::validate_peer_name("user_123").is_ok());

        // Invalid characters
        assert!(ContentValidator::validate_peer_name("alice smith").is_err());
        assert!(ContentValidator::validate_peer_name("user@domain").is_err());
    }

    #[test]
    fn test_security_validation() {
        // Safe content
        assert!(SecurityValidator::is_safe_content("Normal text").is_ok());

        // ANSI escapes
        assert!(SecurityValidator::is_safe_content("\x1b[31mRed\x1b[0m").is_err());

        // Control characters
        assert!(SecurityValidator::is_safe_content("Text\x00").is_err());
    }

    #[test]
    fn test_path_traversal_prevention() {
        // Safe filename
        assert!(ContentValidator::validate_safe_filename("story.txt").is_ok());

        // Path traversal attempts
        assert!(ContentValidator::validate_safe_filename("../etc/passwd").is_err());
        assert!(ContentValidator::validate_safe_filename("subdir/file.txt").is_err());
        assert!(ContentValidator::validate_safe_filename("..\\windows").is_err());
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

    #[test]
    fn test_malformed_utf8_handling() {
        // Test with valid UTF-8
        let valid_utf8 = "Hello 世界";
        assert!(ContentValidator::validate_story_name(valid_utf8).is_ok());

        // The Rust string type guarantees valid UTF-8, so we can't easily test malformed UTF-8
        // But we can test edge cases with special Unicode characters

        // Test with combining characters
        let combining = "e\u{0301}"; // e with acute accent
        assert!(ContentValidator::validate_story_name(combining).is_ok());

        // Test with surrogate-like patterns (valid in UTF-8)
        let special_chars = "Test\u{FEFF}Content"; // BOM character
        let result = ContentValidator::validate_story_name(special_chars);
        assert!(result.is_ok());
    }

    #[test]
    fn test_terminal_injection_payloads() {
        // Test common terminal injection patterns
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

            // Should not contain any escape sequences
            assert!(!sanitized.contains('\x1b'));
            assert_eq!(sanitized, "Normal text  more text");
        }

        // Test control character injection
        let control_chars = "Test\x00\x01\x02\x03\x04\x05Content";
        let sanitized = ContentSanitizer::sanitize_for_display(control_chars);
        assert_eq!(sanitized, "TestContent");

        // Test mixed injection attempts
        let complex_injection = "Start\x1b[31m\x00Evil\x01Content\x1b[0m\x02End";
        let sanitized = ContentSanitizer::sanitize_for_display(complex_injection);
        assert_eq!(sanitized, "StartEvilContentEnd");
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
    fn test_comprehensive_sanitization() {
        let malicious_input = "Hello \x1b[31mWorld\x1b[0m\x00\x01 Test";
        let sanitized = ContentSanitizer::sanitize_for_display(malicious_input);
        assert_eq!(sanitized, "Hello World Test");
    }
}
