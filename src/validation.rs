///! Content validation and sanitization module for P2P-Play
///! 
///! This module provides comprehensive input validation and sanitization functions
///! to prevent security vulnerabilities including:
///! - ANSI escape sequence injection
///! - Terminal control character injection  
///! - Content length abuse/resource exhaustion
///! - Invalid character injection
///! - Binary data injection

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
    TooLong { max_length: usize, actual_length: usize },
    Empty,
    InvalidCharacters { invalid_chars: Vec<char> },
    ContainsControlCharacters,
    ContainsAnsiEscapes,
    ContainsBinaryData,
    InvalidFormat { expected: String },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::TooLong { max_length, actual_length } => {
                write!(f, "Content too long: {} characters (max: {})", actual_length, max_length)
            }
            ValidationError::Empty => write!(f, "Content cannot be empty"),
            ValidationError::InvalidCharacters { invalid_chars } => {
                let chars_str = invalid_chars.iter().collect::<String>();
                write!(f, "Contains invalid characters: {}", chars_str)
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
                write!(f, "Invalid format, expected: {}", expected)
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
                // Start of ANSI escape sequence
                if chars.peek() == Some(&'[') {
                    chars.next(); // consume '['
                    // Skip until we find the end of the sequence
                    while let Some(seq_ch) = chars.next() {
                        if seq_ch.is_ascii_alphabetic() || seq_ch == '~' {
                            break;
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
            .filter(|&ch| ch != '\0' && !ch.is_control() || matches!(ch, ' ' | '\t' | '\n' | '\r'))
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
        ch.is_alphanumeric() 
            || matches!(ch, ' ' | '-' | '_' | '.' | ',' | '!' | '?' | '\'' | '"' | ':' | ';' | '(' | ')' | '\n' | '\r' | '\t')
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
        text.chars().any(|ch| ch.is_control() && !matches!(ch, ' ' | '\t' | '\n' | '\r'))
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
    fn test_control_character_removal() {
        let text_with_controls = "Hello\x00\x01\x02 World\t\n";
        let cleaned = ContentSanitizer::strip_control_characters(text_with_controls);
        assert_eq!(cleaned, "Hello World\t\n");
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
    fn test_comprehensive_sanitization() {
        let malicious_input = "Hello \x1b[31mWorld\x1b[0m\x00\x01 Test";
        let sanitized = ContentSanitizer::sanitize_for_display(malicious_input);
        assert_eq!(sanitized, "Hello World Test");
    }
}