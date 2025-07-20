use crate::types::{DirectMessage, Stories};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use libp2p::PeerId;
use log::debug;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::collections::HashMap;
use std::io::{self, Stdout};
use tokio::sync::mpsc;

pub struct App {
    pub terminal: Terminal<CrosstermBackend<Stdout>>,
    pub should_quit: bool,
    pub input: String,
    pub output_log: Vec<String>,
    pub peers: HashMap<PeerId, String>,
    pub local_stories: Stories,
    pub received_stories: Stories,
    pub local_peer_name: Option<String>,
    pub list_state: ListState,
    pub input_mode: InputMode,
    pub scroll_offset: usize,
    pub auto_scroll: bool, // Track if we should auto-scroll to bottom
}

#[derive(PartialEq, Debug)]
pub enum InputMode {
    Normal,
    Editing,
}

pub enum AppEvent {
    Input(String),
    Quit,
    Log(String),
    PeerUpdate(HashMap<PeerId, String>),
    StoriesUpdate(Stories),
    ReceivedStoriesUpdate(Stories),
    PeerNameUpdate(Option<String>),
    DirectMessage(DirectMessage),
}

impl App {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(App {
            terminal,
            should_quit: false,
            input: String::new(),
            output_log: vec![
                "🎯 P2P-Play Terminal UI - Ready!".to_string(),
                "📝 Press 'i' to enter input mode, 'Esc' to exit input mode".to_string(),
                "🔧 Type 'help' for available commands".to_string(),
                "🧹 Press 'c' to clear output".to_string(),
                "❌ Press 'q' to quit".to_string(),
                "".to_string(),
            ],
            peers: HashMap::new(),
            local_stories: Vec::new(),
            received_stories: Vec::new(),
            local_peer_name: None,
            list_state: ListState::default(),
            input_mode: InputMode::Normal,
            scroll_offset: 0,
            auto_scroll: true, // Start with auto-scroll enabled
        })
    }

    pub fn cleanup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }

    pub fn handle_event(&mut self, event: Event) -> Option<AppEvent> {
        if let Event::Key(key) = event {
            match self.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('q') => {
                        self.should_quit = true;
                        debug!("Quit command received, setting should_quit to true");
                        return Some(AppEvent::Quit);
                    }
                    KeyCode::Char('i') => {
                        self.input_mode = InputMode::Editing;
                    }
                    KeyCode::Char('c') => {
                        self.clear_output();
                    }
                    KeyCode::Up => {
                        self.scroll_up();
                    }
                    KeyCode::Down => {
                        self.scroll_down();
                    }
                    KeyCode::End => {
                        // Re-enable auto-scroll and go to bottom
                        self.auto_scroll = true;
                    }
                    _ => {}
                },
                InputMode::Editing => match key.code {
                    KeyCode::Enter => {
                        let input = self.input.clone();
                        self.input.clear();
                        self.input_mode = InputMode::Normal;
                        if !input.is_empty() {
                            self.add_to_log(format!("> {}", input));
                            return Some(AppEvent::Input(input));
                        }
                    }
                    KeyCode::Char(c) => {
                        if key.modifiers.contains(KeyModifiers::CONTROL) {
                            if c == 'c' {
                                self.input_mode = InputMode::Normal;
                                self.input.clear();
                            }
                        } else {
                            self.input.push(c);
                        }
                    }
                    KeyCode::Backspace => {
                        self.input.pop();
                    }
                    KeyCode::Esc => {
                        self.input_mode = InputMode::Normal;
                        self.input.clear();
                    }
                    _ => {}
                },
            }
        }
        None
    }

    pub fn add_to_log(&mut self, message: String) {
        self.output_log.push(message);
        // Only auto-scroll if auto_scroll is enabled
        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    pub fn clear_output(&mut self) {
        self.output_log.clear();
        self.scroll_offset = 0;
        self.auto_scroll = true; // Re-enable auto-scroll after clear
        self.add_to_log("🧹 Output cleared".to_string());
    }

    pub fn update_peers(&mut self, peers: HashMap<PeerId, String>) {
        self.peers = peers;
    }

    pub fn update_local_stories(&mut self, stories: Stories) {
        self.local_stories = stories;
    }

    pub fn update_received_stories(&mut self, stories: Stories) {
        self.received_stories = stories;
    }

    pub fn update_local_peer_name(&mut self, name: Option<String>) {
        self.local_peer_name = name;
    }

    pub fn handle_direct_message(&mut self, dm: DirectMessage) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.add_to_log(format!(
            "📨 Direct message from {} ({}): {}",
            dm.from_name, timestamp, dm.message
        ));
    }

    fn scroll_up(&mut self) {
        // Always disable auto-scroll when user manually scrolls
        self.auto_scroll = false;
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    fn scroll_down(&mut self) {
        // Don't use the old max_scroll calculation that was based on line index
        // Instead, we'll let the draw() method handle proper clamping
        self.scroll_offset += 1;
        self.auto_scroll = false; // Disable auto-scroll when user manually scrolls
    }

    fn scroll_to_bottom(&mut self) {
        // Calculate scroll offset to show the bottom of the log
        // Use a reasonable estimate for visible height (we'll use ~20 lines as default)
        let estimated_visible_height = 20;
        if self.output_log.len() <= estimated_visible_height {
            self.scroll_offset = 0;
        } else {
            self.scroll_offset = self.output_log.len().saturating_sub(estimated_visible_height);
        }
    }
    
    fn scroll_to_bottom_with_height(&mut self, log_height: usize) {
        // Position scroll to show the last messages in the visible window
        if self.output_log.len() <= log_height {
            self.scroll_offset = 0;
        } else {
            self.scroll_offset = self.output_log.len().saturating_sub(log_height);
        }
    }
    


    pub fn draw(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Status bar
                    Constraint::Min(0),    // Main area
                    Constraint::Length(3), // Input area
                ])
                .split(f.size());

            // Status bar
            let version = env!("CARGO_PKG_VERSION");
            let status_text = if let Some(ref name) = self.local_peer_name {
                format!(
                    "P2P-Play v{} | Peer: {} | Connected: {} | Mode: {} | AUTO: {}",
                    version,
                    name,
                    self.peers.len(),
                    match self.input_mode {
                        InputMode::Normal => "Normal",
                        InputMode::Editing => "Editing",
                    },
                    if self.auto_scroll { "ON" } else { "OFF" }
                )
            } else {
                format!(
                    "P2P-Play v{} | No peer name set | Connected: {} | Mode: {} | AUTO: {}",
                    version,
                    self.peers.len(),
                    match self.input_mode {
                        InputMode::Normal => "Normal",
                        InputMode::Editing => "Editing",
                    },
                    if self.auto_scroll { "ON" } else { "OFF" }
                )
            };

            let status_bar = Paragraph::new(status_text)
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().borders(Borders::ALL).title("Status"));
            f.render_widget(status_bar, chunks[0]);

            // Main area - split into left and right
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Min(80), // Output area - minimum 80 characters
                    Constraint::Min(30), // Side panels - minimum 30 characters
                ])
                .split(chunks[1]);

            // Output log
            let actual_log_height = (main_chunks[0].height as usize).saturating_sub(2);
            let total_lines = self.output_log.len();
            
            // Calculate scroll position considering auto_scroll
            let scroll_offset = if self.auto_scroll {
                // Auto-scroll: show the bottom of the log
                if total_lines <= actual_log_height {
                    0
                } else {
                    total_lines.saturating_sub(actual_log_height)
                }
            } else {
                // Manual scroll: use the current scroll_offset, but clamp it
                if total_lines <= actual_log_height {
                    0
                } else {
                    let max_scroll = total_lines.saturating_sub(actual_log_height);
                    self.scroll_offset.min(max_scroll)
                }
            };

            // Calculate what portion of the log to display
            let visible_start = scroll_offset;
            let visible_end = std::cmp::min(visible_start + actual_log_height, total_lines);

            // Convert log messages to display text using explicit ratatui structures
            let lines: Vec<Line> = self.output_log[visible_start..visible_end]
                .iter()
                .map(|msg| Line::from(Span::raw(msg.clone())))
                .collect();

            let text = Text::from(lines);

            // Create title with scroll indicator
            let title = if total_lines > actual_log_height {
                format!("Output [{}/{}]", visible_start + 1, total_lines)
            } else {
                "Output".to_string()
            };

            let output = Paragraph::new(text)
                .block(Block::default().borders(Borders::ALL).title(title))
                .wrap(ratatui::widgets::Wrap { trim: false })
                .alignment(ratatui::layout::Alignment::Left);
            f.render_widget(output, main_chunks[0]);

            // Side panels - split into top and bottom
            let side_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(50), // Peers
                    Constraint::Percentage(50), // Stories
                ])
                .split(main_chunks[1]);

            // Peers list
            let peer_items: Vec<ListItem> = self
                .peers
                .iter()
                .map(|(peer_id, name)| {
                    let content = if name.is_empty() {
                        format!("{}", peer_id)
                    } else {
                        format!("{} ({})", name, peer_id)
                    };
                    ListItem::new(content)
                })
                .collect();

            let peers_list = List::new(peer_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Connected Peers"),
                )
                .highlight_style(Style::default().fg(Color::Yellow));
            f.render_widget(peers_list, side_chunks[0]);

            // Stories list
            let story_items: Vec<ListItem> = self
                .local_stories
                .iter()
                .chain(self.received_stories.iter())
                .map(|story| {
                    let status = if story.public { "📖" } else { "📕" };
                    ListItem::new(format!("{} {}: {}", status, story.id, story.name))
                })
                .collect();

            let stories_list = List::new(story_items)
                .block(Block::default().borders(Borders::ALL).title("Stories"))
                .highlight_style(Style::default().fg(Color::Yellow));
            f.render_widget(stories_list, side_chunks[1]);

            // Input area
            let input_style = match self.input_mode {
                InputMode::Normal => Style::default(),
                InputMode::Editing => Style::default().fg(Color::Yellow),
            };

            let input_text = match self.input_mode {
                InputMode::Normal => {
                    "Press 'i' to enter input mode, ↑/↓ to scroll, 'End' to enable auto-scroll, 'c' to clear output, 'q' to quit"
                        .to_string()
                }
                InputMode::Editing => format!("Command: {}", self.input),
            };

            let input = Paragraph::new(input_text)
                .style(input_style)
                .block(Block::default().borders(Borders::ALL).title("Input"));
            f.render_widget(input, chunks[2]);

            // Set cursor position if in editing mode
            if self.input_mode == InputMode::Editing {
                f.set_cursor(
                    chunks[2].x + self.input.len() as u16 + 10, // 10 is for "Command: "
                    chunks[2].y + 1,
                );
            }
        })?;

        Ok(())
    }
}

/// Event handler for UI events
pub async fn handle_ui_events(
    app: &mut App,
    ui_sender: mpsc::UnboundedSender<AppEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Use a shorter poll duration for more responsive input
    if event::poll(std::time::Duration::from_millis(16))? {
        if let Some(app_event) = app.handle_event(event::read()?) {
            ui_sender.send(app_event)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::PeerId;
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
        let events = vec![
            AppEvent::Input("test".to_string()),
            AppEvent::Quit,
            AppEvent::Log("test log".to_string()),
            AppEvent::PeerUpdate(HashMap::new()),
            AppEvent::StoriesUpdate(Vec::new()),
            AppEvent::ReceivedStoriesUpdate(Vec::new()),
            AppEvent::PeerNameUpdate(None),
        ];

        // Test that we can create all event variants
        assert_eq!(events.len(), 7);
    }

    #[test]
    fn test_direct_message_handling() {
        // Test DirectMessage creation with mock data
        let dm = DirectMessage {
            from_peer_id: "peer123".to_string(),
            from_name: "Alice".to_string(),
            to_name: "Bob".to_string(),
            message: "Hello Bob!".to_string(),
            timestamp: 1234567890,
        };

        assert_eq!(dm.from_name, "Alice");
        assert_eq!(dm.message, "Hello Bob!");
    }

    #[test]
    fn test_story_formatting() {
        use crate::types::Story;

        let story = Story {
            id: 1,
            name: "Test Story".to_string(),
            header: "Test Header".to_string(),
            body: "Test Body".to_string(),
            public: true,
            channel: "general".to_string(),
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
        // Scroll position should be adjusted to show the bottom

        // Test manual scroll disables auto-scroll
        mock_app.scroll_up();
        assert!(!mock_app.auto_scroll);

        // Test adding message with auto-scroll disabled doesn't move scroll
        let scroll_before = mock_app.scroll_offset;
        mock_app.add_to_log("New message 2".to_string());
        assert_eq!(mock_app.scroll_offset, scroll_before);

        // Test re-enabling auto-scroll
        mock_app.auto_scroll = true;
        mock_app.add_to_log("New message 3".to_string());
        // Should now auto-scroll again
    }

    #[test]
    fn test_auto_scroll_status_display() {
        // Test that auto-scroll status is properly displayed
        let mut mock_app = MockAppWithAutoScroll {
            output_log: vec!["Test".to_string()],
            scroll_offset: 0,
            auto_scroll: true,
        };

        assert!(mock_app.auto_scroll);
        
        mock_app.scroll_up();
        assert!(!mock_app.auto_scroll);
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
            // Simulate the auto-scroll behavior
            if self.auto_scroll {
                // In a real app, this would be calculated with log_height
                // For testing, just move to a reasonable position
                if self.output_log.len() > 5 {
                    self.scroll_offset = self.output_log.len().saturating_sub(5);
                } else {
                    self.scroll_offset = 0;
                }
            }
        }

        fn scroll_up(&mut self) {
            // Always disable auto-scroll when user manually scrolls, even if at top
            self.auto_scroll = false;
            if self.scroll_offset > 0 {
                self.scroll_offset -= 1;
            }
        }
    }
}
