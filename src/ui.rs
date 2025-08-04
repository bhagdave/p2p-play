use crate::types::{Channels, DirectMessage, Stories};
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

#[cfg(windows)]
use std::sync::{Arc, Mutex};
#[cfg(windows)]
use std::time::Instant;

#[cfg(windows)]
static LAST_KEY_EVENT: std::sync::LazyLock<
    Arc<Mutex<Option<(crossterm::event::KeyEvent, Instant)>>>,
> = std::sync::LazyLock::new(|| Arc::new(Mutex::new(None)));

#[cfg(windows)]
fn should_process_key_event(event: &crossterm::event::KeyEvent) -> bool {
    let mut last_event_guard = LAST_KEY_EVENT.lock().unwrap();

    if let Some((last_event, last_time)) = *last_event_guard {
        // Skip if same key pressed within 190ms (duplicate detection)
        if last_event.code == event.code
            && last_event.modifiers == event.modifiers
            && last_time.elapsed() < std::time::Duration::from_millis(190)
        {
            return false;
        }
    }

    *last_event_guard = Some((*event, Instant::now()));
    true
}

pub struct App {
    pub terminal: Terminal<CrosstermBackend<Stdout>>,
    pub should_quit: bool,
    pub input: String,
    pub output_log: Vec<String>,
    pub peers: HashMap<PeerId, String>,
    pub stories: Stories,
    pub channels: Channels,
    pub view_mode: ViewMode,
    pub local_peer_name: Option<String>,
    pub list_state: ListState,
    pub input_mode: InputMode,
    pub scroll_offset: usize,
    pub auto_scroll: bool, // Track if we should auto-scroll to bottom
}

#[derive(PartialEq, Debug, Clone)]
pub enum ViewMode {
    Channels,
    Stories(String), // Selected channel name
}

#[derive(PartialEq, Debug, Clone)]
pub enum InputMode {
    Normal,
    Editing,
    CreatingStory {
        step: StoryCreationStep,
        partial_story: PartialStory,
    },
}

#[derive(PartialEq, Debug, Clone)]
pub enum StoryCreationStep {
    Name,
    Header,
    Body,
    Channel,
}

#[derive(PartialEq, Debug, Clone)]
pub struct PartialStory {
    pub name: Option<String>,
    pub header: Option<String>,
    pub body: Option<String>,
    pub channel: Option<String>,
}

pub enum AppEvent {
    Input(String),
    Quit,
}

impl App {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // UI initialization code that's difficult to test without a real terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(io::stdout());
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
            stories: Vec::new(),
            channels: Vec::new(),
            view_mode: ViewMode::Channels,
            local_peer_name: None,
            list_state: ListState::default(),
            input_mode: InputMode::Normal,
            scroll_offset: 0,
            auto_scroll: true, // Start with auto-scroll enabled
        })
    }

    pub fn cleanup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Terminal cleanup code that's difficult to test
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }

    pub fn handle_event(&mut self, event: Event) -> Option<AppEvent> {
        if let Event::Key(key) = event {
            #[cfg(windows)]
            {
                if !should_process_key_event(&key) {
                    return None; // Skip duplicate event
                }
            }
            match &self.input_mode {
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
                        // Navigate list items when in channels/stories view, otherwise scroll output
                        match self.view_mode {
                            ViewMode::Channels => {
                                self.navigate_list_up();
                            }
                            ViewMode::Stories(_) => {
                                self.navigate_list_up();
                            }
                        }
                        // Always allow output scrolling as well
                        self.scroll_up();
                    }
                    KeyCode::Down => {
                        // Navigate list items when in channels/stories view, otherwise scroll output
                        match self.view_mode {
                            ViewMode::Channels => {
                                self.navigate_list_down();
                            }
                            ViewMode::Stories(_) => {
                                self.navigate_list_down();
                            }
                        }
                        // Always allow output scrolling as well
                        self.scroll_down();
                    }
                    KeyCode::End => {
                        // Re-enable auto-scroll and go to bottom
                        self.auto_scroll = true;
                        // Reset scroll offset to ensure clean transition to auto-scroll
                        self.scroll_offset = 0;
                    }
                    KeyCode::Enter => {
                        // Handle navigation between channels and stories
                        if let ViewMode::Channels = self.view_mode {
                            if let Some(channel_name) = self.get_selected_channel() {
                                self.enter_channel(channel_name.to_string());
                            }
                        }
                    }
                    KeyCode::Esc => {
                        // Return to channels view if in stories view
                        if matches!(self.view_mode, ViewMode::Stories(_)) {
                            self.return_to_channels();
                        }
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
                InputMode::CreatingStory {
                    step,
                    partial_story,
                } => {
                    match key.code {
                        KeyCode::Esc => {
                            self.cancel_story_creation();
                        }
                        KeyCode::Enter => {
                            let input = self.input.trim().to_string();
                            self.input.clear();

                            let mut new_partial = partial_story.clone();
                            let mut next_step = None;

                            match step {
                                StoryCreationStep::Name => {
                                    if input.is_empty() {
                                        self.add_to_log(
                                            "❌ Story name cannot be empty. Please try again:"
                                                .to_string(),
                                        );
                                        return None;
                                    }
                                    new_partial.name = Some(input);
                                    next_step = Some(StoryCreationStep::Header);
                                    self.add_to_log("✅ Story name saved".to_string());
                                    self.add_to_log("📄 Enter story header:".to_string());
                                }
                                StoryCreationStep::Header => {
                                    if input.is_empty() {
                                        self.add_to_log(
                                            "❌ Story header cannot be empty. Please try again:"
                                                .to_string(),
                                        );
                                        return None;
                                    }
                                    new_partial.header = Some(input);
                                    next_step = Some(StoryCreationStep::Body);
                                    self.add_to_log("✅ Story header saved".to_string());
                                    self.add_to_log("📖 Enter story body:".to_string());
                                }
                                StoryCreationStep::Body => {
                                    if input.is_empty() {
                                        self.add_to_log(
                                            "❌ Story body cannot be empty. Please try again:"
                                                .to_string(),
                                        );
                                        return None;
                                    }
                                    new_partial.body = Some(input);
                                    next_step = Some(StoryCreationStep::Channel);
                                    self.add_to_log("✅ Story body saved".to_string());
                                    self.add_to_log(
                                        "📂 Enter channel (or press Enter for 'general'):"
                                            .to_string(),
                                    );
                                }
                                StoryCreationStep::Channel => {
                                    let channel = if input.is_empty() {
                                        "general".to_string()
                                    } else {
                                        input
                                    };
                                    new_partial.channel = Some(channel);

                                    // Story creation complete - create the command string
                                    if let (Some(name), Some(header), Some(body), Some(ch)) = (
                                        &new_partial.name,
                                        &new_partial.header,
                                        &new_partial.body,
                                        &new_partial.channel,
                                    ) {
                                        let create_command =
                                            format!("create s {}|{}|{}|{}", name, header, body, ch);
                                        self.input_mode = InputMode::Normal;
                                        self.add_to_log("✅ Story creation complete!".to_string());
                                        return Some(AppEvent::Input(create_command));
                                    }
                                }
                            }

                            // Update to next step if not complete
                            if let Some(step) = next_step {
                                self.input_mode = InputMode::CreatingStory {
                                    step,
                                    partial_story: new_partial,
                                };
                            }
                        }
                        KeyCode::Char(c) => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                if c == 'c' {
                                    self.cancel_story_creation();
                                }
                            } else {
                                self.input.push(c);
                            }
                        }
                        KeyCode::Backspace => {
                            self.input.pop();
                        }
                        _ => {}
                    }
                }
            }
        }
        None
    }

    pub fn add_to_log(&mut self, message: String) {
        self.output_log.push(message);
        // Note: scroll position is handled automatically in draw() method
        // when auto_scroll is enabled, so no need to call scroll_to_bottom() here
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

    pub fn update_stories(&mut self, stories: Stories) {
        self.stories = stories;
    }

    pub fn update_local_peer_name(&mut self, name: Option<String>) {
        self.local_peer_name = name;
    }

    pub fn update_channels(&mut self, channels: Channels) {
        self.channels = channels;
        // Initialize selection to first channel if in channels view and we have channels
        if matches!(self.view_mode, ViewMode::Channels) && !self.channels.is_empty() && self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
    }

    pub fn enter_channel(&mut self, channel_name: String) {
        self.view_mode = ViewMode::Stories(channel_name);
        // Reset list selection when entering stories view
        self.list_state.select(Some(0));
    }

    pub fn return_to_channels(&mut self) {
        self.view_mode = ViewMode::Channels;
        // Reset list selection when returning to channels view
        if !self.channels.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    pub fn navigate_list_up(&mut self) {
        let list_len = match self.view_mode {
            ViewMode::Channels => self.channels.len(),
            ViewMode::Stories(ref channel_name) => {
                self.stories.iter()
                    .filter(|story| story.channel == *channel_name)
                    .count()
            }
        };
        
        if list_len > 0 {
            let current = self.list_state.selected().unwrap_or(0);
            let new_index = if current == 0 { list_len - 1 } else { current - 1 };
            self.list_state.select(Some(new_index));
        }
    }

    pub fn navigate_list_down(&mut self) {
        let list_len = match self.view_mode {
            ViewMode::Channels => self.channels.len(),
            ViewMode::Stories(ref channel_name) => {
                self.stories.iter()
                    .filter(|story| story.channel == *channel_name)
                    .count()
            }
        };
        
        if list_len > 0 {
            let current = self.list_state.selected().unwrap_or(0);
            let new_index = if current >= list_len - 1 { 0 } else { current + 1 };
            self.list_state.select(Some(new_index));
        }
    }

    pub fn get_selected_channel(&self) -> Option<&str> {
        // Check if we're in channels view and have channels available
        if matches!(self.view_mode, ViewMode::Channels) && !self.channels.is_empty() {
            // Use list_state.selected() to get the current selection
            if let Some(selected_index) = self.list_state.selected() {
                if selected_index < self.channels.len() {
                    Some(&self.channels[selected_index].name)
                } else {
                    // Fallback to first channel if index is out of bounds
                    Some(&self.channels[0].name)
                }
            } else {
                // No selection, default to first channel
                Some(&self.channels[0].name)
            }
        } else {
            None
        }
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

    pub fn start_story_creation(&mut self) {
        self.input_mode = InputMode::CreatingStory {
            step: StoryCreationStep::Name,
            partial_story: PartialStory {
                name: None,
                header: None,
                body: None,
                channel: None,
            },
        };
        self.input.clear();
        self.add_to_log("📖 Starting interactive story creation...".to_string());
        self.add_to_log("📝 Enter story name (or Esc to cancel):".to_string());
    }

    pub fn cancel_story_creation(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input.clear();
        self.add_to_log("❌ Story creation cancelled".to_string());
    }

    pub fn get_current_step_prompt(&self) -> String {
        match &self.input_mode {
            InputMode::CreatingStory { step, .. } => match step {
                StoryCreationStep::Name => "📝 Enter story name:".to_string(),
                StoryCreationStep::Header => "📄 Enter story header:".to_string(),
                StoryCreationStep::Body => "📖 Enter story body:".to_string(),
                StoryCreationStep::Channel => {
                    "📂 Enter channel (or press Enter for 'general'):".to_string()
                }
            },
            _ => "".to_string(),
        }
    }

    /// Calculate the current scroll position based on auto_scroll state and available height
    /// Returns the scroll offset that should be used for display
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

    fn scroll_up(&mut self) {
        // If auto-scroll is currently enabled, we need to transition smoothly
        // by setting scroll_offset to the current auto-scroll position first
        if self.auto_scroll {
            // Estimate available height (terminal height minus UI elements)
            // Conservative estimate: assume terminal is at least 24 lines,
            // minus 3 for status, 3 for input, 2 for borders = ~16 lines for output
            let estimated_height = 16;
            self.scroll_offset = self.calculate_current_scroll_position(estimated_height);
        }

        // Disable auto-scroll when user manually scrolls
        self.auto_scroll = false;

        // Now perform the scroll up operation
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    fn scroll_down(&mut self) {
        // If auto-scroll is currently enabled, we need to transition smoothly
        // by setting scroll_offset to the current auto-scroll position first
        if self.auto_scroll {
            // Estimate available height (terminal height minus UI elements)
            let estimated_height = 16;
            self.scroll_offset = self.calculate_current_scroll_position(estimated_height);
        }

        // Disable auto-scroll when user manually scrolls
        self.auto_scroll = false;

        // Now perform the scroll down operation
        // We'll let the draw() method handle proper clamping of the scroll_offset
        self.scroll_offset += 1;
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
                        InputMode::CreatingStory { .. } => "Creating Story",
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
                        InputMode::CreatingStory { .. } => "Creating Story",
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
                    let peer_id_str = peer_id.to_string();
                    // Use 20 characters instead of 8 to ensure uniqueness between peers with similar prefixes
                    let peer_id_display = if peer_id_str.len() >= 20 { &peer_id_str[..20] } else { &peer_id_str };

                    let content = if name.is_empty() {
                        format!("{}", peer_id)
                    } else if name.starts_with("Peer_") && name.contains(&peer_id.to_string()) {
                        // This is a default name we assigned (contains full peer ID), show truncated version
                        format!("Peer_{} [{}]", peer_id_display, peer_id_display)
                    } else {
                        // This is a real name the peer set (or a custom name that starts with "Peer_")
                        format!("{} ({})", name, peer_id_display)
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

            // Channels/Stories list - display based on view mode
            let (list_items, list_title) = match &self.view_mode {
                ViewMode::Channels => {
                    let channel_items: Vec<ListItem> = self
                        .channels
                        .iter()
                        .map(|channel| {
                            // Count stories in this channel
                            let story_count = self.stories.iter()
                                .filter(|story| story.channel == channel.name)
                                .count();
                            ListItem::new(format!("📂 {} ({} stories) - {}", 
                                channel.name, story_count, channel.description))
                        })
                        .collect();
                    (channel_items, "Channels (Press Enter to view stories)".to_string())
                }
                ViewMode::Stories(selected_channel) => {
                    let story_items: Vec<ListItem> = self
                        .stories
                        .iter()
                        .filter(|story| story.channel == *selected_channel)
                        .map(|story| {
                            let status = if story.public { "📖" } else { "📕" };
                            ListItem::new(format!("{} {}: {}", status, story.id, story.name))
                        })
                        .collect();
                    (story_items, format!("Stories in '{}' (Press Esc to return to channels)", selected_channel))
                }
            };

            let list = List::new(list_items)
                .block(Block::default().borders(Borders::ALL).title(list_title))
                .highlight_style(Style::default().fg(Color::Yellow));
            f.render_stateful_widget(list, side_chunks[1], &mut self.list_state);

            // Input area
            let input_style = match self.input_mode {
                InputMode::Normal => Style::default(),
                InputMode::Editing => Style::default().fg(Color::Yellow),
                InputMode::CreatingStory { .. } => Style::default().fg(Color::Green),
            };

            let input_text = match &self.input_mode {
                InputMode::Normal => {
                    match &self.view_mode {
                        ViewMode::Channels => "Press 'i' to enter input mode, Enter to view channel stories, ↑/↓ to scroll, 'c' to clear output, 'q' to quit".to_string(),
                        ViewMode::Stories(_) => "Press 'i' to enter input mode, Esc to return to channels, ↑/↓ to scroll, 'c' to clear output, 'q' to quit".to_string(),
                    }
                }
                InputMode::Editing => format!("Command: {}", self.input),
                InputMode::CreatingStory { step, .. } => {
                    let prompt = match step {
                        StoryCreationStep::Name => "📝 Story Name",
                        StoryCreationStep::Header => "📄 Story Header", 
                        StoryCreationStep::Body => "📖 Story Body",
                        StoryCreationStep::Channel => "📂 Channel (Enter for 'general')",
                    };
                    format!("{}: {}", prompt, self.input)
                }
            };

            let input = Paragraph::new(input_text)
                .style(input_style)
                .block(Block::default().borders(Borders::ALL).title("Input"));
            f.render_widget(input, chunks[2]);

            // Set cursor position if in editing mode or creating story
            match &self.input_mode {
                InputMode::Editing => {
                    f.set_cursor(
                        chunks[2].x + self.input.len() as u16 + 10, // 10 is for "Command: "
                        chunks[2].y + 1,
                    );
                }
                InputMode::CreatingStory { step, .. } => {
                    // Use display width constants instead of .len() to handle emoji widths correctly
                    let prefix_len = match step {
                        StoryCreationStep::Name => 15,        // "📝 Story Name: " display width
                        StoryCreationStep::Header => 17,      // "📄 Story Header: " display width
                        StoryCreationStep::Body => 14,        // "📖 Story Body: " display width
                        StoryCreationStep::Channel => 34,     // "📂 Channel (Enter for 'general'): " display width
                    };
                    f.set_cursor(
                        chunks[2].x + self.input.len() as u16 + prefix_len as u16 + 1,
                        chunks[2].y + 1,
                    );
                }
                _ => {}
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
    #[cfg(windows)]
    let poll_timeout = std::time::Duration::from_millis(80); // Slower polling on Windows

    #[cfg(not(windows))]
    let poll_timeout = std::time::Duration::from_millis(16); // Keep fast polling on Unix    

    if event::poll(poll_timeout)? {
        if let Some(app_event) = app.handle_event(event::read()?) {
            ui_sender.send(app_event)?;
        }
    }
    Ok(())
}
