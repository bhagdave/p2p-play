use libp2p::PeerId;
use p2p_play::types::{Channel, Conversation, DirectMessage};
use p2p_play::ui::{AppEvent, InputMode, PartialStory, StoryCreationStep, ViewMode};
use std::collections::HashMap;

#[test]
fn test_app_creation() {
    // We can't test the full app creation due to terminal requirements
    // but we can test the data structures
    let mut peers = HashMap::new();
    let peer_id = PeerId::random();
    peers.insert(peer_id, "test_peer".to_string());

    // Test that the data structures work as expected
    assert_eq!(peers.len(), 1);
    assert_eq!(peers.get(&peer_id), Some(&"test_peer".to_string()));
}

#[test]
fn test_input_mode() {
    let normal_mode = InputMode::Normal;
    let editing_mode = InputMode::Editing;

    assert_eq!(normal_mode, InputMode::Normal);
    assert_eq!(editing_mode, InputMode::Editing);
    assert_ne!(normal_mode, editing_mode);
}

#[test]
fn test_app_event_variants() {
    let dm = DirectMessage {
        from_peer_id: "peer123".to_string(),
        from_name: "Alice".to_string(),
        to_peer_id: "peer456".to_string(),
        to_name: "Bob".to_string(),
        message: "Hello!".to_string(),
        timestamp: 1234567890,
        is_outgoing: false,
    };

    let events = [
        AppEvent::Input("test".to_string()),
        AppEvent::Quit,
        AppEvent::StoryViewed {
            story_id: 1,
            channel: "general".to_string(),
        },
        AppEvent::DirectMessage(dm),
    ];

    // Test that we can create all event variants
    assert_eq!(events.len(), 4);
}

#[test]
fn test_story_creation_states() {
    // Test story creation step enumeration
    let steps = [
        StoryCreationStep::Name,
        StoryCreationStep::Header,
        StoryCreationStep::Body,
        StoryCreationStep::Channel,
    ];
    assert_eq!(steps.len(), 4);

    // Test partial story creation
    let partial = PartialStory {
        name: Some("Test Story".to_string()),
        header: Some("Test Header".to_string()),
        body: None,
        channel: None,
    };
    assert_eq!(partial.name, Some("Test Story".to_string()));
    assert_eq!(partial.header, Some("Test Header".to_string()));
    assert!(partial.body.is_none());
    assert!(partial.channel.is_none());
}

#[test]
fn test_input_mode_story_creation() {
    let normal_mode = InputMode::Normal;
    let editing_mode = InputMode::Editing;
    let creating_mode = InputMode::CreatingStory {
        step: StoryCreationStep::Name,
        partial_story: PartialStory {
            name: None,
            header: None,
            body: None,
            channel: None,
        },
    };

    assert_eq!(normal_mode, InputMode::Normal);
    assert_eq!(editing_mode, InputMode::Editing);
    assert_ne!(normal_mode, creating_mode);
    assert_ne!(editing_mode, creating_mode);

    // Test that story creation mode holds the right data
    if let InputMode::CreatingStory {
        step,
        partial_story,
    } = creating_mode
    {
        assert_eq!(step, StoryCreationStep::Name);
        assert!(partial_story.name.is_none());
    } else {
        panic!("Expected CreatingStory mode");
    }
}

#[test]
fn test_direct_message_handling() {
    // Test DirectMessage creation with mock data
    let dm = DirectMessage {
        from_peer_id: "peer123".to_string(),
        from_name: "Alice".to_string(),
        to_peer_id: "peer456".to_string(),
        to_name: "Bob".to_string(),
        message: "Hello Bob!".to_string(),
        timestamp: 1234567890,
        is_outgoing: false,
    };

    assert_eq!(dm.from_name, "Alice");
    assert_eq!(dm.message, "Hello Bob!");
}

#[test]
fn test_direct_message_storage_and_unread_tracking() {
    // Create a mock App structure with our new fields
    struct MockApp {
        direct_messages: Vec<DirectMessage>,
        unread_message_count: usize,
    }

    impl MockApp {
        fn new() -> Self {
            Self {
                direct_messages: Vec::new(),
                unread_message_count: 0,
            }
        }

        fn handle_direct_message(&mut self, dm: DirectMessage) {
            self.direct_messages.push(dm);
            self.unread_message_count += 1;
        }

        fn mark_messages_as_read(&mut self) {
            self.unread_message_count = 0;
        }
    }

    let mut app = MockApp::new();

    // Initially no messages
    assert_eq!(app.direct_messages.len(), 0);
    assert_eq!(app.unread_message_count, 0);

    // Add first message
    let dm1 = DirectMessage {
        from_peer_id: "peer123".to_string(),
        from_name: "Alice".to_string(),
        to_peer_id: "peer789".to_string(),
        to_name: "Bob".to_string(),
        message: "Hello Bob!".to_string(),
        timestamp: 1234567890,
        is_outgoing: false,
    };
    app.handle_direct_message(dm1);

    assert_eq!(app.direct_messages.len(), 1);
    assert_eq!(app.unread_message_count, 1);

    // Add second message
    let dm2 = DirectMessage {
        from_peer_id: "peer456".to_string(),
        from_name: "Charlie".to_string(),
        to_peer_id: "peer789".to_string(),
        to_name: "Bob".to_string(),
        message: "Hi there!".to_string(),
        timestamp: 1234567900,
        is_outgoing: false,
    };
    app.handle_direct_message(dm2);

    assert_eq!(app.direct_messages.len(), 2);
    assert_eq!(app.unread_message_count, 2);

    // Mark as read
    app.mark_messages_as_read();
    assert_eq!(app.direct_messages.len(), 2); // Messages still stored
    assert_eq!(app.unread_message_count, 0); // But unread count reset

    // Verify message content
    assert_eq!(app.direct_messages[0].from_name, "Alice");
    assert_eq!(app.direct_messages[1].from_name, "Charlie");
}

#[test]
fn test_story_formatting() {
    use p2p_play::types::Story;

    let story = Story {
        id: 1,
        name: "Test Story".to_string(),
        header: "Test Header".to_string(),
        body: "Test Body".to_string(),
        public: true,
        channel: "general".to_string(),
        created_at: 1640995200,
        auto_share: None, // Fixed timestamp for testing (2022-01-01)
    };

    let status = if story.public { "📖" } else { "📕" };
    let formatted = format!("{} {}: {}", status, story.id, story.name);

    assert_eq!(formatted, "📖 1: Test Story");
}

#[test]
fn test_version_display_in_status_bar() {
    // Test that the version is properly included in status bar text
    let version = env!("CARGO_PKG_VERSION");

    // Test status bar with peer name
    let status_with_peer = format!(
        "P2P-Play v{} | Peer: {} | Connected: {} | Mode: {}",
        version, "TestPeer", 2, "Normal"
    );
    assert!(status_with_peer.contains("P2P-Play v"));
    assert!(status_with_peer.contains(version));

    // Test status bar without peer name
    let status_without_peer = format!(
        "P2P-Play v{} | No peer name set | Connected: {} | Mode: {}",
        version, 0, "Editing"
    );
    assert!(status_without_peer.contains("P2P-Play v"));
    assert!(status_without_peer.contains(version));
}

#[test]
fn test_clear_output_functionality() {
    // Create a mock app structure for testing clear output
    let mut mock_app = MockApp {
        output_log: vec![
            "Initial message 1".to_string(),
            "Initial message 2".to_string(),
            "Initial message 3".to_string(),
        ],
        scroll_offset: 2,
    };

    // Verify initial state
    assert_eq!(mock_app.output_log.len(), 3);
    assert_eq!(mock_app.scroll_offset, 2);

    // Test clear output
    mock_app.clear_output();

    // Should have only the "Output cleared" message
    assert_eq!(mock_app.output_log.len(), 1);
    assert_eq!(mock_app.output_log[0], "🧹 Output cleared");
    assert_eq!(mock_app.scroll_offset, 0);
}

#[test]
fn test_clear_output_when_empty() {
    // Test clearing when output log is empty
    let mut mock_app = MockApp {
        output_log: vec![],
        scroll_offset: 0,
    };

    mock_app.clear_output();

    // Should have only the "Output cleared" message
    assert_eq!(mock_app.output_log.len(), 1);
    assert_eq!(mock_app.output_log[0], "🧹 Output cleared");
    assert_eq!(mock_app.scroll_offset, 0);
}

#[test]
fn test_auto_scroll_functionality() {
    // Create a mock app structure for testing auto-scroll
    let mut mock_app = MockAppWithAutoScroll {
        output_log: vec![
            "Initial message 1".to_string(),
            "Initial message 2".to_string(),
        ],
        scroll_offset: 0,
        auto_scroll: true,
    };

    // Test initial state
    assert_eq!(mock_app.output_log.len(), 2);
    assert_eq!(mock_app.scroll_offset, 0);
    assert!(mock_app.auto_scroll);

    // Test adding a message with auto-scroll enabled
    mock_app.add_to_log("New message 1".to_string());
    assert_eq!(mock_app.output_log.len(), 3);
    // Note: scroll position is now handled in draw() method, not in add_to_log()

    // Test manual scroll disables auto-scroll
    mock_app.scroll_up();
    assert!(!mock_app.auto_scroll);

    // Test adding message with auto-scroll disabled
    mock_app.add_to_log("New message 2".to_string());
    assert_eq!(mock_app.output_log.len(), 4);
    // Scroll position doesn't change since it's handled in draw()

    // Test re-enabling auto-scroll
    mock_app.auto_scroll = true;
    mock_app.add_to_log("New message 3".to_string());
    assert_eq!(mock_app.output_log.len(), 5);
    // Auto-scroll positioning happens in draw() method
}

#[test]
fn test_auto_scroll_to_manual_scroll_transition() {
    // Test the smooth transition from auto-scroll to manual scroll
    let mut mock_app = MockAppWithAutoScrollTransition {
        output_log: vec![
            "Line 1".to_string(),
            "Line 2".to_string(),
            "Line 3".to_string(),
            "Line 4".to_string(),
            "Line 5".to_string(),
            "Line 6".to_string(),
            "Line 7".to_string(),
            "Line 8".to_string(),
            "Line 9".to_string(),
            "Line 10".to_string(),
        ],
        scroll_offset: 0,
        auto_scroll: true,
    };

    // Initially auto-scroll is enabled
    assert!(mock_app.auto_scroll);
    assert_eq!(mock_app.scroll_offset, 0);

    // When we have more lines than display height, auto-scroll should show bottom
    let display_height = 5;
    let expected_auto_scroll_position = mock_app.output_log.len().saturating_sub(display_height);
    assert_eq!(expected_auto_scroll_position, 5); // Should show lines 6-10

    // When user scrolls up while auto-scroll is active, it should:
    // 1. Set scroll_offset to current auto-scroll position
    // 2. Disable auto-scroll
    // 3. Then perform the scroll operation
    mock_app.scroll_up_with_height(display_height);

    // After scroll_up: auto-scroll should be disabled and scroll_offset should be set appropriately
    assert!(!mock_app.auto_scroll);
    // Should start from the auto-scroll position (5) and then scroll up by 1, so 4
    assert_eq!(mock_app.scroll_offset, 4);

    // Further manual scroll should work normally
    mock_app.scroll_up_with_height(display_height);
    assert!(!mock_app.auto_scroll);
    assert_eq!(mock_app.scroll_offset, 3);

    // Test scroll down transition as well
    let mut mock_app2 = MockAppWithAutoScrollTransition {
        output_log: vec![
            "Line 1".to_string(),
            "Line 2".to_string(),
            "Line 3".to_string(),
            "Line 4".to_string(),
            "Line 5".to_string(),
            "Line 6".to_string(),
            "Line 7".to_string(),
            "Line 8".to_string(),
            "Line 9".to_string(),
            "Line 10".to_string(),
        ],
        scroll_offset: 0,
        auto_scroll: true,
    };

    mock_app2.scroll_down_with_height(display_height);
    assert!(!mock_app2.auto_scroll);
    // Should start from auto-scroll position (5) and scroll down by 1, so 6
    // But since max scroll is 5, it should be clamped to 5 in practice (handled by draw method)
    assert_eq!(mock_app2.scroll_offset, 6);
}

#[test]
fn test_calculate_current_scroll_position() {
    // Test the scroll position calculation method
    let mut mock_app = MockAppWithCalculation {
        output_log: vec![
            "Line 1".to_string(),
            "Line 2".to_string(),
            "Line 3".to_string(),
            "Line 4".to_string(),
            "Line 5".to_string(),
            "Line 6".to_string(),
            "Line 7".to_string(),
            "Line 8".to_string(),
            "Line 9".to_string(),
            "Line 10".to_string(),
        ],
        scroll_offset: 2,
        auto_scroll: true,
    };

    let display_height = 5;

    // When auto_scroll is true, should return bottom position
    let pos = mock_app.calculate_current_scroll_position(display_height);
    assert_eq!(pos, 5); // 10 lines - 5 display height = 5

    // When auto_scroll is false, should return clamped scroll_offset
    mock_app.auto_scroll = false;
    let pos = mock_app.calculate_current_scroll_position(display_height);
    assert_eq!(pos, 2); // Should use scroll_offset

    // Test with scroll_offset beyond max
    mock_app.scroll_offset = 10; // Beyond max
    let pos = mock_app.calculate_current_scroll_position(display_height);
    assert_eq!(pos, 5); // Should be clamped to max (10 - 5 = 5)

    // Test when total lines <= display height
    mock_app.output_log = vec!["Line 1".to_string(), "Line 2".to_string()];
    mock_app.auto_scroll = true;
    let pos = mock_app.calculate_current_scroll_position(display_height);
    assert_eq!(pos, 0); // Should return 0 when everything fits

    mock_app.auto_scroll = false;
    mock_app.scroll_offset = 10;
    let pos = mock_app.calculate_current_scroll_position(display_height);
    assert_eq!(pos, 0); // Should return 0 when everything fits, regardless of scroll_offset
}

// Mock App structure for testing since we can't create a full App with terminal
struct MockApp {
    output_log: Vec<String>,
    scroll_offset: usize,
}

impl MockApp {
    fn clear_output(&mut self) {
        self.output_log.clear();
        self.scroll_offset = 0;
        self.add_to_log("🧹 Output cleared".to_string());
    }

    fn add_to_log(&mut self, message: String) {
        self.output_log.push(message);
        // Preserve scroll_offset = 0 if the log was cleared
        if self.output_log.len() == 1 && self.output_log[0] == "🧹 Output cleared" {
            self.scroll_offset = 0;
        } else {
            self.scroll_offset = self.output_log.len().saturating_sub(1);
        }
    }
}

// Mock App structure for testing auto-scroll functionality
struct MockAppWithAutoScroll {
    output_log: Vec<String>,
    scroll_offset: usize,
    auto_scroll: bool,
}

impl MockAppWithAutoScroll {
    fn add_to_log(&mut self, message: String) {
        self.output_log.push(message);
        // Note: In the real implementation, scroll position is handled in draw() method
        // For testing, we don't simulate the auto-scroll here since it's handled elsewhere
    }

    fn scroll_up(&mut self) {
        // Always disable auto-scroll when user manually scrolls, even if at top
        self.auto_scroll = false;
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }
}

// Mock App structure for testing smooth transition from auto-scroll to manual scroll
struct MockAppWithAutoScrollTransition {
    output_log: Vec<String>,
    scroll_offset: usize,
    auto_scroll: bool,
}

impl MockAppWithAutoScrollTransition {
    fn calculate_current_scroll_position(&self, available_height: usize) -> usize {
        let total_lines = self.output_log.len();

        if self.auto_scroll {
            // Auto-scroll: show the bottom of the log
            if total_lines <= available_height {
                0
            } else {
                total_lines.saturating_sub(available_height)
            }
        } else {
            // Manual scroll: use the current scroll_offset, but clamp it
            if total_lines <= available_height {
                0
            } else {
                let max_scroll = total_lines.saturating_sub(available_height);
                self.scroll_offset.min(max_scroll)
            }
        }
    }

    fn scroll_up_with_height(&mut self, available_height: usize) {
        // If auto-scroll is currently enabled, we need to transition smoothly
        if self.auto_scroll {
            self.scroll_offset = self.calculate_current_scroll_position(available_height);
        }

        // Disable auto-scroll when user manually scrolls
        self.auto_scroll = false;

        // Now perform the scroll up operation
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    fn scroll_down_with_height(&mut self, available_height: usize) {
        // If auto-scroll is currently enabled, we need to transition smoothly
        if self.auto_scroll {
            self.scroll_offset = self.calculate_current_scroll_position(available_height);
        }

        // Disable auto-scroll when user manually scrolls
        self.auto_scroll = false;

        // Now perform the scroll down operation
        self.scroll_offset += 1;
    }
}

// Mock App structure for testing the scroll position calculation method
struct MockAppWithCalculation {
    output_log: Vec<String>,
    scroll_offset: usize,
    auto_scroll: bool,
}

impl MockAppWithCalculation {
    fn calculate_current_scroll_position(&self, available_height: usize) -> usize {
        let total_lines = self.output_log.len();

        if self.auto_scroll {
            // Auto-scroll: show the bottom of the log
            if total_lines <= available_height {
                0
            } else {
                total_lines.saturating_sub(available_height)
            }
        } else {
            // Manual scroll: use the current scroll_offset, but clamp it
            if total_lines <= available_height {
                0
            } else {
                let max_scroll = total_lines.saturating_sub(available_height);
                self.scroll_offset.min(max_scroll)
            }
        }
    }
}

#[test]
fn test_auto_scroll_re_enable_functionality() {
    // Create a mock app that simulates the auto-scroll re-enable behavior
    struct MockAppAutoScrollReEnable {
        output_log: Vec<String>,
        scroll_offset: usize,
        auto_scroll: bool,
    }

    impl MockAppAutoScrollReEnable {
        fn new() -> Self {
            Self {
                output_log: vec![
                    "Line 1".to_string(),
                    "Line 2".to_string(),
                    "Line 3".to_string(),
                    "Line 4".to_string(),
                    "Line 5".to_string(),
                ],
                scroll_offset: 0,
                auto_scroll: true,
            }
        }

        fn scroll_up(&mut self) {
            // Simulate transition from auto-scroll to manual scroll
            if self.auto_scroll {
                let estimated_height = 3; // Simulate small display area
                self.scroll_offset = self.calculate_current_scroll_position(estimated_height);
            }
            self.auto_scroll = false;
            if self.scroll_offset > 0 {
                self.scroll_offset -= 1;
            }
        }

        fn re_enable_auto_scroll(&mut self) {
            // This simulates pressing 'End' key - the fix we implemented
            self.auto_scroll = true;
            self.scroll_offset = 0; // Reset scroll offset for clean transition
        }

        fn add_to_log(&mut self, message: String) {
            self.output_log.push(message);
        }

        fn calculate_current_scroll_position(&self, available_height: usize) -> usize {
            let total_lines = self.output_log.len();

            if self.auto_scroll {
                // Auto-scroll: show the bottom of the log
                if total_lines <= available_height {
                    0
                } else {
                    total_lines.saturating_sub(available_height)
                }
            } else {
                // Manual scroll: use the current scroll_offset, but clamp it
                if total_lines <= available_height {
                    0
                } else {
                    let max_scroll = total_lines.saturating_sub(available_height);
                    self.scroll_offset.min(max_scroll)
                }
            }
        }

        fn get_visible_range(&self, available_height: usize) -> (usize, usize) {
            let scroll_pos = self.calculate_current_scroll_position(available_height);
            let visible_start = scroll_pos;
            let visible_end =
                std::cmp::min(visible_start + available_height, self.output_log.len());
            (visible_start, visible_end)
        }
    }

    let mut app = MockAppAutoScrollReEnable::new();
    let display_height = 3;

    // Initially in auto-scroll mode, should show bottom lines
    assert!(app.auto_scroll);
    assert_eq!(app.scroll_offset, 0);
    let (start, end) = app.get_visible_range(display_height);
    assert_eq!(start, 2); // Should show lines 2, 3, 4 (indices 2, 3, 4)
    assert_eq!(end, 5);

    // User does manual scrolling - this disables auto-scroll
    app.scroll_up();
    assert!(!app.auto_scroll); // Auto-scroll should be disabled
    assert_eq!(app.scroll_offset, 1); // Should be scrolled up from bottom

    // User scrolls up more
    app.scroll_up();
    assert!(!app.auto_scroll);
    assert_eq!(app.scroll_offset, 0); // Should be at top now

    // User re-enables auto-scroll (presses 'End' key)
    app.re_enable_auto_scroll();
    assert!(app.auto_scroll); // Auto-scroll should be re-enabled
    assert_eq!(app.scroll_offset, 0); // Scroll offset should be reset

    // Add a new message - should be visible at bottom when auto-scroll is on
    app.add_to_log("New message".to_string());

    // With auto-scroll enabled, should show the bottom including the new message
    let (start, end) = app.get_visible_range(display_height);
    // Now we have 6 lines total, display_height = 3, so should show lines 3, 4, 5 (indices 3, 4, 5)
    assert_eq!(start, 3);
    assert_eq!(end, 6);

    // The visible content should include the new message
    let visible_lines: Vec<&String> = app.output_log[start..end].iter().collect();
    assert!(visible_lines.contains(&&"New message".to_string()));
}

#[test]
fn test_view_mode_variants() {
    // Test ViewMode enum variants
    let channels_view = ViewMode::Channels;
    let stories_view = ViewMode::Stories("general".to_string());

    assert_eq!(channels_view, ViewMode::Channels);
    assert_eq!(stories_view, ViewMode::Stories("general".to_string()));
    assert_ne!(channels_view, stories_view);
}

#[test]
fn test_view_mode_navigation() {
    // Test the view mode navigation logic
    let mut view_mode = ViewMode::Channels;

    // Test entering a channel
    view_mode = ViewMode::Stories("general".to_string());
    match view_mode {
        ViewMode::Stories(channel_name) => {
            assert_eq!(channel_name, "general");
        }
        _ => panic!("Expected Stories view mode"),
    }

    // Test returning to channels view
    view_mode = ViewMode::Channels;
    match view_mode {
        ViewMode::Channels => {
            // This is expected
        }
        _ => panic!("Expected Channels view mode"),
    }
}

#[test]
fn test_channel_display_formatting() {
    // Test channel display formatting similar to existing story formatting test
    let channel = Channel {
        name: "general".to_string(),
        description: "Default general discussion channel".to_string(),
        created_by: "system".to_string(),
        created_at: 1640995200, // Fixed timestamp for testing
    };

    // Simulate the formatting used in the UI
    let story_count = 5; // Mock story count
    let formatted = format!(
        "📂 {} ({} stories) - {}",
        channel.name, story_count, channel.description
    );

    assert_eq!(
        formatted,
        "📂 general (5 stories) - Default general discussion channel"
    );
}

#[test]
fn test_channel_navigation_state() {
    // Test a mock app structure that includes the new view mode functionality
    struct MockAppWithChannelNav {
        view_mode: ViewMode,
        channels: Vec<Channel>,
    }

    impl MockAppWithChannelNav {
        fn new() -> Self {
            Self {
                view_mode: ViewMode::Channels,
                channels: vec![
                    Channel {
                        name: "general".to_string(),
                        description: "Default channel".to_string(),
                        created_by: "system".to_string(),
                        created_at: 1640995200,
                    },
                    Channel {
                        name: "tech".to_string(),
                        description: "Technology discussions".to_string(),
                        created_by: "admin".to_string(),
                        created_at: 1640995300,
                    },
                ],
            }
        }

        fn enter_channel(&mut self, channel_name: String) {
            self.view_mode = ViewMode::Stories(channel_name);
        }

        fn return_to_channels(&mut self) {
            self.view_mode = ViewMode::Channels;
        }

        fn get_selected_channel(&self) -> Option<&str> {
            if matches!(self.view_mode, ViewMode::Channels) && !self.channels.is_empty() {
                Some(&self.channels[0].name)
            } else {
                None
            }
        }
    }

    let mut app = MockAppWithChannelNav::new();

    // Initially should be in channels view
    assert_eq!(app.view_mode, ViewMode::Channels);
    assert_eq!(app.get_selected_channel(), Some("general"));

    // Test entering a channel
    app.enter_channel("tech".to_string());
    match app.view_mode {
        ViewMode::Stories(ref channel_name) => {
            assert_eq!(channel_name, "tech");
        }
        _ => panic!("Expected Stories view mode"),
    }
    assert_eq!(app.get_selected_channel(), None); // Should return None when in stories view

    // Test returning to channels
    app.return_to_channels();
    assert_eq!(app.view_mode, ViewMode::Channels);
    assert_eq!(app.get_selected_channel(), Some("general"));
}

#[test]
fn test_conversation_view_modes() {
    // Test the new conversation-related ViewMode variants
    let conversations_view = ViewMode::Conversations;
    let conversation_view = ViewMode::ConversationView("peer123".to_string());

    assert_eq!(conversations_view, ViewMode::Conversations);
    assert_eq!(
        conversation_view,
        ViewMode::ConversationView("peer123".to_string())
    );
    assert_ne!(conversations_view, conversation_view);
}

#[test]
fn test_conversation_navigation_flow() {
    // Test the complete navigation flow between conversations
    struct MockAppWithConversations {
        view_mode: ViewMode,
        conversations: Vec<Conversation>,
        selected_index: Option<usize>,
    }

    impl MockAppWithConversations {
        fn new() -> Self {
            Self {
                view_mode: ViewMode::Conversations,
                conversations: vec![
                    Conversation {
                        peer_id: "peer1".to_string(),
                        peer_name: "Alice".to_string(),
                        messages: vec![],
                        unread_count: 2,
                        last_activity: 3000,
                    },
                    Conversation {
                        peer_id: "peer2".to_string(),
                        peer_name: "Bob".to_string(),
                        messages: vec![],
                        unread_count: 0,
                        last_activity: 2000,
                    },
                ],
                selected_index: Some(0),
            }
        }

        fn get_selected_conversation(&self) -> Option<&Conversation> {
            if matches!(self.view_mode, ViewMode::Conversations) {
                if let Some(index) = self.selected_index {
                    self.conversations.get(index)
                } else {
                    None
                }
            } else {
                None
            }
        }

        fn enter_conversation(&mut self, peer_id: String) {
            self.view_mode = ViewMode::ConversationView(peer_id);
        }

        fn return_to_conversations(&mut self) {
            self.view_mode = ViewMode::Conversations;
            self.selected_index = Some(0);
        }

        fn cycle_view_mode(&mut self) {
            self.view_mode = match &self.view_mode {
                ViewMode::Channels => ViewMode::Conversations,
                ViewMode::Conversations => ViewMode::Channels,
                ViewMode::Stories(_) => ViewMode::Channels,
                ViewMode::ConversationView(_) => ViewMode::Conversations,
            };
            self.selected_index = Some(0);
        }

        fn navigate_list_down(&mut self) {
            let list_len = match self.view_mode {
                ViewMode::Conversations => self.conversations.len(),
                _ => 0,
            };
            if list_len > 0 {
                let current = self.selected_index.unwrap_or(0);
                let new_index = if current >= list_len - 1 {
                    0 // Wrap around to beginning
                } else {
                    current + 1
                };
                self.selected_index = Some(new_index);
            }
        }

        fn navigate_list_up(&mut self) {
            let list_len = match self.view_mode {
                ViewMode::Conversations => self.conversations.len(),
                _ => 0,
            };
            if list_len > 0 {
                let current = self.selected_index.unwrap_or(0);
                let new_index = if current == 0 {
                    list_len - 1 // Wrap around to end
                } else {
                    current - 1
                };
                self.selected_index = Some(new_index);
            }
        }
    }

    let mut app = MockAppWithConversations::new();

    // Initially should be in conversations view with first conversation selected
    assert_eq!(app.view_mode, ViewMode::Conversations);
    assert_eq!(app.selected_index, Some(0));

    let selected_conversation = app.get_selected_conversation().unwrap();
    assert_eq!(selected_conversation.peer_name, "Alice");
    assert_eq!(selected_conversation.unread_count, 2);

    // Test navigation within conversations list
    app.navigate_list_down();
    let selected_conversation = app.get_selected_conversation().unwrap();
    assert_eq!(selected_conversation.peer_name, "Bob");
    assert_eq!(selected_conversation.unread_count, 0);

    // Test wrap-around navigation
    app.navigate_list_down();
    let selected_conversation = app.get_selected_conversation().unwrap();
    assert_eq!(selected_conversation.peer_name, "Alice"); // Should wrap to first

    // Test entering a conversation
    app.enter_conversation("peer2".to_string());
    match app.view_mode {
        ViewMode::ConversationView(ref peer_id) => {
            assert_eq!(peer_id, "peer2");
        }
        _ => panic!("Expected ConversationView mode"),
    }
    assert!(app.get_selected_conversation().is_none()); // Should return None in conversation view

    // Test returning to conversations list
    app.return_to_conversations();
    assert_eq!(app.view_mode, ViewMode::Conversations);
    assert_eq!(app.selected_index, Some(0));

    let selected_conversation = app.get_selected_conversation().unwrap();
    assert_eq!(selected_conversation.peer_name, "Alice");
}

#[test]
fn test_conversation_app_events() {
    // Test the new ConversationViewed event
    let conversation_event = AppEvent::ConversationViewed {
        peer_id: "peer123".to_string(),
    };

    match conversation_event {
        AppEvent::ConversationViewed { peer_id } => {
            assert_eq!(peer_id, "peer123");
        }
        _ => panic!("Expected ConversationViewed event"),
    }
}

#[test]
fn test_conversation_display_formatting() {
    // Test conversation display formatting
    let conversation = Conversation {
        peer_id: "peer123".to_string(),
        peer_name: "Alice".to_string(),
        messages: vec![],
        unread_count: 3,
        last_activity: 1640995200,
    };

    // Format as it would appear in the UI
    let formatted_with_unread = if conversation.unread_count > 0 {
        format!(
            "{} ({} unread)",
            conversation.peer_name, conversation.unread_count
        )
    } else {
        conversation.peer_name.clone()
    };

    assert_eq!(formatted_with_unread, "Alice (3 unread)");

    // Test with no unread messages
    let conversation_read = Conversation {
        peer_id: "peer456".to_string(),
        peer_name: "Bob".to_string(),
        messages: vec![],
        unread_count: 0,
        last_activity: 1640995300,
    };

    let formatted_read = if conversation_read.unread_count > 0 {
        format!(
            "{} ({} unread)",
            conversation_read.peer_name, conversation_read.unread_count
        )
    } else {
        conversation_read.peer_name.clone()
    };

    assert_eq!(formatted_read, "Bob");
}

#[test]
fn test_conversation_sorting_by_activity() {
    // Test that conversations are properly sorted by last activity
    let mut conversations = vec![
        Conversation {
            peer_id: "peer1".to_string(),
            peer_name: "Alice".to_string(),
            messages: vec![],
            unread_count: 1,
            last_activity: 1000,
        },
        Conversation {
            peer_id: "peer2".to_string(),
            peer_name: "Bob".to_string(),
            messages: vec![],
            unread_count: 2,
            last_activity: 3000,
        },
        Conversation {
            peer_id: "peer3".to_string(),
            peer_name: "Charlie".to_string(),
            messages: vec![],
            unread_count: 0,
            last_activity: 2000,
        },
    ];

    // Sort by last activity (most recent first) - as the database query does
    conversations.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));

    assert_eq!(conversations[0].peer_name, "Bob"); // Most recent
    assert_eq!(conversations[1].peer_name, "Charlie"); // Middle
    assert_eq!(conversations[2].peer_name, "Alice"); // Oldest
}

#[test]
fn test_empty_conversations_handling() {
    // Test handling when no conversations exist
    struct MockEmptyConversations {
        view_mode: ViewMode,
        conversations: Vec<Conversation>,
        selected_index: Option<usize>,
    }

    impl MockEmptyConversations {
        fn new() -> Self {
            Self {
                view_mode: ViewMode::Conversations,
                conversations: vec![],
                selected_index: None,
            }
        }

        fn get_selected_conversation(&self) -> Option<&Conversation> {
            if matches!(self.view_mode, ViewMode::Conversations) {
                if let Some(index) = self.selected_index {
                    self.conversations.get(index)
                } else {
                    None
                }
            } else {
                None
            }
        }

        fn navigate_list_down(&mut self) {
            let list_len = self.conversations.len();
            if list_len > 0 {
                let current = self.selected_index.unwrap_or(0);
                let new_index = if current >= list_len - 1 {
                    0
                } else {
                    current + 1
                };
                self.selected_index = Some(new_index);
            }
            // If list is empty, do nothing
        }
    }

    let mut app = MockEmptyConversations::new();

    assert_eq!(app.view_mode, ViewMode::Conversations);
    assert_eq!(app.conversations.len(), 0);
    assert_eq!(app.selected_index, None);
    assert!(app.get_selected_conversation().is_none());

    // Navigation should be safe with empty list
    app.navigate_list_down();
    assert_eq!(app.selected_index, None);
    assert!(app.get_selected_conversation().is_none());
}
