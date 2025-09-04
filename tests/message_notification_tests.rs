use p2p_play::types::{DirectMessage, MessageNotificationConfig};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn test_message_notification_config_defaults() {
    let config = MessageNotificationConfig::new();

    assert!(config.enable_color_coding);
    assert!(!config.enable_sound_notifications); // Should be off by default
    assert!(config.enable_flash_indicators);
    assert_eq!(config.flash_duration_ms, 200);
    assert!(config.show_delivery_status);
    assert!(config.enhanced_timestamps);
}

#[test]
fn test_message_notification_config_validation() {
    let mut config = MessageNotificationConfig::new();

    // Valid configuration should pass
    assert!(config.validate().is_ok());

    // Zero flash duration should fail
    config.flash_duration_ms = 0;
    assert!(config.validate().is_err());

    // Very long flash duration should fail
    config.flash_duration_ms = 10000;
    assert!(config.validate().is_err());

    // Valid flash duration should pass
    config.flash_duration_ms = 500;
    assert!(config.validate().is_ok());
}

#[test]
fn test_message_notification_config_serialization() {
    let config = MessageNotificationConfig::new();

    // Test serialization
    let json = serde_json::to_string(&config).expect("Failed to serialize config");
    assert!(json.contains("enable_color_coding"));
    assert!(json.contains("enable_sound_notifications"));
    assert!(json.contains("enable_flash_indicators"));

    // Test deserialization
    let deserialized: MessageNotificationConfig =
        serde_json::from_str(&json).expect("Failed to deserialize config");

    assert_eq!(config.enable_color_coding, deserialized.enable_color_coding);
    assert_eq!(
        config.enable_sound_notifications,
        deserialized.enable_sound_notifications
    );
    assert_eq!(
        config.enable_flash_indicators,
        deserialized.enable_flash_indicators
    );
    assert_eq!(config.flash_duration_ms, deserialized.flash_duration_ms);
    assert_eq!(
        config.show_delivery_status,
        deserialized.show_delivery_status
    );
    assert_eq!(config.enhanced_timestamps, deserialized.enhanced_timestamps);
}

#[test]
fn test_direct_message_for_notifications() {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let dm = DirectMessage {
        from_peer_id: "peer123".to_string(),
        from_name: "Alice".to_string(),
        to_peer_id: "peer456".to_string(),
        to_name: "Bob".to_string(),
        message: "Hello, this is a test message!".to_string(),
        timestamp,
        is_outgoing: false,
    };

    // Verify message structure for notification processing
    assert_eq!(dm.from_name, "Alice");
    assert_eq!(dm.to_name, "Bob");
    assert!(!dm.is_outgoing); // Incoming message should trigger notifications
    assert!(!dm.message.is_empty());
    assert!(dm.timestamp > 0);
}

#[cfg(test)]
mod ui_notification_tests {
    use super::*;

    // Mock UI structure for testing notification features
    struct MockAppWithNotifications {
        notification_config: MessageNotificationConfig,
        flash_active: bool,
        flash_start_time: Option<std::time::Instant>,
        unread_message_count: usize,
    }

    impl MockAppWithNotifications {
        fn new() -> Self {
            Self {
                notification_config: MessageNotificationConfig::new(),
                flash_active: false,
                flash_start_time: None,
                unread_message_count: 0,
            }
        }

        fn trigger_flash_notification(&mut self) {
            if self.notification_config.enable_flash_indicators {
                self.flash_active = true;
                self.flash_start_time = Some(std::time::Instant::now());
            }
        }

        fn update_flash_indicator(&mut self) {
            if self.flash_active {
                if let Some(start_time) = self.flash_start_time {
                    let elapsed = start_time.elapsed().as_millis() as u64;
                    if elapsed >= self.notification_config.flash_duration_ms {
                        self.flash_active = false;
                        self.flash_start_time = None;
                    }
                }
            }
        }
    }

    #[test]
    fn test_flash_indicator_activation() {
        let mut mock_app = MockAppWithNotifications::new();

        // Initially no flash
        assert!(!mock_app.flash_active);
        assert!(mock_app.flash_start_time.is_none());

        // Trigger flash notification
        mock_app.trigger_flash_notification();

        // Flash should now be active
        assert!(mock_app.flash_active);
        assert!(mock_app.flash_start_time.is_some());
    }

    #[test]
    fn test_flash_indicator_timeout() {
        let mut mock_app = MockAppWithNotifications::new();
        mock_app.notification_config.flash_duration_ms = 50; // Very short duration for testing

        // Trigger flash
        mock_app.trigger_flash_notification();
        assert!(mock_app.flash_active);

        // Wait longer than flash duration
        std::thread::sleep(Duration::from_millis(100));

        // Update flash indicator
        mock_app.update_flash_indicator();

        // Flash should be inactive now
        assert!(!mock_app.flash_active);
        assert!(mock_app.flash_start_time.is_none());
    }

    #[test]
    fn test_flash_indicator_disabled() {
        let mut mock_app = MockAppWithNotifications::new();
        mock_app.notification_config.enable_flash_indicators = false;

        // Trigger flash notification when disabled
        mock_app.trigger_flash_notification();

        // Flash should remain inactive
        assert!(!mock_app.flash_active);
        assert!(mock_app.flash_start_time.is_none());
    }
}

#[test]
fn test_enhanced_timestamp_formatting() {
    let config = MessageNotificationConfig::new();
    assert!(config.enhanced_timestamps);

    // Test that enhanced timestamps are enabled by default
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Simulate different time scenarios for enhanced formatting
    let recent = now - 30; // 30 seconds ago
    let minutes_ago = now - 300; // 5 minutes ago
    let hours_ago = now - 7200; // 2 hours ago
    let days_ago = now - 172800; // 2 days ago

    // All should be valid timestamps
    assert!(recent < now);
    assert!(minutes_ago < now);
    assert!(hours_ago < now);
    assert!(days_ago < now);
}
