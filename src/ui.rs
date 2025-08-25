use crate::errors::UIResult;
use crate::types::{Channels, ConversationManager, DirectMessage, Icons, Stories, Story};
use crate::validation::ContentSanitizer;
use chrono::{DateTime, Local};
use crossterm::{
    event::{
        self, Event, KeyCode, KeyModifiers, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
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
use std::time::{Duration, UNIX_EPOCH};
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
    pub unread_counts: HashMap<String, usize>, // Channel name -> unread count
    pub view_mode: ViewMode,
    pub local_peer_name: Option<String>,
    pub local_peer_id: Option<String>,
    pub list_state: ListState,
    pub input_mode: InputMode,
    pub scroll_offset: usize,
    pub auto_scroll: bool, // Track if we should auto-scroll to bottom
    pub network_health: Option<crate::network_circuit_breakers::NetworkHealthSummary>,
    pub direct_messages: Vec<DirectMessage>, // Store direct messages separately
    pub unread_message_count: usize,         // Track unread direct messages
    pub conversation_manager: ConversationManager, // Manage conversations
    // Enhanced input features
    pub input_history: Vec<String>, // Command history for Up/Down navigation
    pub history_index: Option<usize>, // Current position in history
    pub last_message_sender: Option<String>, // Track last sender for quick reply
}

#[derive(PartialEq, Debug, Clone)]
pub enum ViewMode {
    Channels,
    Stories(String),            // Selected channel name
    Conversations,              // Conversation list view
    ConversationThread(String), // Thread view for specific peer
}

#[derive(PartialEq, Debug, Clone)]
pub enum InputMode {
    Normal,
    Editing,
    CreatingStory {
        step: StoryCreationStep,
        partial_story: PartialStory,
    },
    QuickReply {
        target_peer: String,
    },
    MessageComposition {
        target_peer: String,
        lines: Vec<String>,
        current_line: String,
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
    StoryViewed { story_id: usize, channel: String },
    DirectMessage(DirectMessage),
    EnterMessageComposition { target_peer: String },
}

impl App {
    pub fn new() -> UIResult<Self> {
        // UI initialization code that's difficult to test without a real terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;

        Ok(App {
            terminal,
            should_quit: false,
            input: String::new(),
            output_log: vec![
                format!("{} P2P-Play Terminal UI - Ready!", Icons::target()),
                format!(
                    "{} Press 'i' to enter input mode, 'Esc' to exit input mode",
                    Icons::memo()
                ),
                format!("{} Type 'help' for available commands", Icons::wrench()),
                format!("{} Press 'c' to clear output", Icons::broom()),
                format!("{} Press Ctrl+S to toggle auto-scroll", Icons::check()),
                format!("{} Press 'q' to quit", Icons::cross()),
                "".to_string(),
            ],
            peers: HashMap::new(),
            stories: Vec::new(),
            channels: Vec::new(),
            unread_counts: HashMap::new(),
            view_mode: ViewMode::Channels,
            local_peer_name: None,
            local_peer_id: None,
            list_state: ListState::default(),
            input_mode: InputMode::Normal,
            scroll_offset: 0,
            auto_scroll: true, // Start with auto-scroll enabled
            network_health: None,
            direct_messages: Vec::new(),
            unread_message_count: 0,
            conversation_manager: ConversationManager::new(),
            // Enhanced input features
            input_history: Vec::new(),
            history_index: None,
            last_message_sender: None,
        })
    }

    pub fn cleanup(&mut self) -> UIResult<()> {
        // Terminal cleanup code that's difficult to test
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            PopKeyboardEnhancementFlags,
            LeaveAlternateScreen
        )?;
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
                        // Only handle output scrolling with Up/Down keys
                        self.scroll_up();
                    }
                    KeyCode::Down => {
                        // Only handle output scrolling with Up/Down keys
                        self.scroll_down();
                    }
                    KeyCode::Left => {
                        // Navigate list items with Left/Right keys
                        match self.view_mode {
                            ViewMode::Channels => {
                                self.navigate_list_up();
                            }
                            ViewMode::Stories(_) => {
                                self.navigate_list_up();
                            }
                            ViewMode::Conversations => {
                                self.navigate_list_up();
                            }
                            ViewMode::ConversationThread(_) => {
                                // In thread view, Left key could go back to conversation list
                                self.view_mode = ViewMode::Conversations;
                                self.list_state.select(Some(0));
                            }
                        }
                    }
                    KeyCode::Right => {
                        // Navigate list items with Left/Right keys
                        match self.view_mode {
                            ViewMode::Channels => {
                                self.navigate_list_down();
                            }
                            ViewMode::Stories(_) => {
                                self.navigate_list_down();
                            }
                            ViewMode::Conversations => {
                                self.navigate_list_down();
                            }
                            ViewMode::ConversationThread(_) => {
                                // In thread view, Right key scrolls messages
                                self.navigate_list_down();
                            }
                        }
                    }
                    KeyCode::End => {
                        // Re-enable auto-scroll and go to bottom
                        self.auto_scroll = true;
                        // Reset scroll offset to ensure clean transition to auto-scroll
                        self.scroll_offset = 0;
                    }
                    KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Toggle auto-scroll on/off
                        self.auto_scroll = !self.auto_scroll;
                        if self.auto_scroll {
                            // Reset scroll offset when re-enabling auto-scroll to go to bottom
                            self.scroll_offset = 0;
                        }

                        // Add visual feedback to the log
                        self.add_to_log(format!(
                            "{} Auto-scroll {} (Ctrl+S)",
                            Icons::check(),
                            if self.auto_scroll {
                                "enabled"
                            } else {
                                "disabled"
                            }
                        ));
                    }
                    KeyCode::ScrollLock | KeyCode::F(12) => {
                        // Keep ScrollLock and F12 as fallback options for systems that support them
                        // Toggle auto-scroll on/off
                        self.auto_scroll = !self.auto_scroll;
                        if self.auto_scroll {
                            // Reset scroll offset when re-enabling auto-scroll to go to bottom
                            self.scroll_offset = 0;
                        }

                        // Add visual feedback to the log
                        self.add_to_log(format!(
                            "{} Auto-scroll {} ({})",
                            Icons::check(),
                            if self.auto_scroll {
                                "enabled"
                            } else {
                                "disabled"
                            },
                            match key.code {
                                KeyCode::ScrollLock => "ScrollLock",
                                KeyCode::F(12) => "F12",
                                _ => "key",
                            }
                        ));
                    }
                    KeyCode::Enter => {
                        // Handle navigation between channels and stories, or view story content
                        match &self.view_mode {
                            ViewMode::Channels => {
                                if let Some(channel_name) = self.get_selected_channel() {
                                    self.enter_channel(channel_name.to_string());
                                }
                            }
                            ViewMode::Stories(_) => {
                                if let Some(story) = self.get_selected_story() {
                                    self.display_story_content(&story);
                                    // Return event to mark story as read asynchronously
                                    return Some(AppEvent::StoryViewed {
                                        story_id: story.id,
                                        channel: story.channel.clone(),
                                    });
                                }
                            }
                            ViewMode::Conversations => {
                                // Enter conversation thread view
                                if let Some(conversation) = self.get_selected_conversation() {
                                    self.enter_conversation_thread(conversation.peer_id.clone());
                                }
                            }
                            ViewMode::ConversationThread(_) => {
                                // In thread view, Enter could scroll or do nothing
                                // For now, just scroll down
                                self.navigate_list_down();
                            }
                        }
                    }
                    KeyCode::Esc => {
                        // Return to appropriate parent view
                        match &self.view_mode {
                            ViewMode::Stories(_) => {
                                self.return_to_channels();
                            }
                            ViewMode::ConversationThread(_) => {
                                self.return_to_conversations();
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Tab => {
                        // Switch between conversations view and channels view
                        match &self.view_mode {
                            ViewMode::Channels | ViewMode::Stories(_) => {
                                self.view_mode = ViewMode::Conversations;
                                self.list_state.select(Some(0));
                            }
                            ViewMode::Conversations | ViewMode::ConversationThread(_) => {
                                self.view_mode = ViewMode::Channels;
                                self.list_state.select(Some(0));
                            }
                        }
                    }
                    KeyCode::Char('r') => {
                        // Quick reply to last message sender
                        if let Some(ref last_sender) = self.last_message_sender.clone() {
                            self.input_mode = InputMode::QuickReply {
                                target_peer: last_sender.clone(),
                            };
                            self.add_to_log(format!(
                                "{} Quick reply to {}",
                                crate::types::Icons::envelope(),
                                last_sender
                            ));
                        } else {
                            self.add_to_log(format!(
                                "{} No recent messages to reply to",
                                crate::types::Icons::cross()
                            ));
                        }
                    }
                    KeyCode::Char('m') => {
                        // Enter message composition mode (will be handled via input command)
                        self.input_mode = InputMode::Editing;
                        self.input = "compose ".to_string();
                    }
                    _ => {}
                },
                InputMode::Editing => match key.code {
                    KeyCode::Enter => {
                        let input = self.input.clone();
                        self.input.clear();
                        self.input_mode = InputMode::Normal;
                        self.history_index = None; // Reset history navigation
                        if !input.is_empty() {
                            // Add to history (avoid duplicates)
                            if self.input_history.is_empty()
                                || self.input_history.last() != Some(&input)
                            {
                                self.input_history.push(input.clone());
                                // Keep history size manageable
                                if self.input_history.len() > 50 {
                                    self.input_history.remove(0);
                                }
                            }
                            self.add_to_log(format!("> {input}"));
                            return Some(AppEvent::Input(input));
                        }
                    }
                    KeyCode::Up => {
                        // Navigate up in command history
                        if !self.input_history.is_empty() {
                            match self.history_index {
                                None => {
                                    // Start from the end
                                    self.history_index = Some(self.input_history.len() - 1);
                                    self.input =
                                        self.input_history[self.input_history.len() - 1].clone();
                                }
                                Some(idx) if idx > 0 => {
                                    self.history_index = Some(idx - 1);
                                    self.input = self.input_history[idx - 1].clone();
                                }
                                _ => {
                                    // Already at the beginning, stay there
                                }
                            }
                        }
                    }
                    KeyCode::Down => {
                        // Navigate down in command history
                        if let Some(idx) = self.history_index {
                            if idx < self.input_history.len() - 1 {
                                self.history_index = Some(idx + 1);
                                self.input = self.input_history[idx + 1].clone();
                            } else {
                                // Clear input when going past the end
                                self.history_index = None;
                                self.input.clear();
                            }
                        }
                    }
                    KeyCode::Tab => {
                        // Auto-complete peer names for msg commands
                        if self.input.starts_with("msg ") {
                            self.try_autocomplete_peer_name();
                        }
                    }
                    KeyCode::Char(c) => {
                        if key.modifiers.contains(KeyModifiers::CONTROL) {
                            match c {
                                'c' => {
                                    self.input_mode = InputMode::Normal;
                                    self.input.clear();
                                    self.history_index = None;
                                }
                                'l' => {
                                    // Clear current input line
                                    self.input.clear();
                                    self.history_index = None;
                                }
                                _ => {}
                            }
                        } else {
                            self.input.push(c);
                            // Reset history navigation when user types
                            self.history_index = None;
                        }
                    }
                    KeyCode::Backspace => {
                        self.input.pop();
                        // Reset history navigation when user modifies input
                        self.history_index = None;
                    }
                    KeyCode::Esc => {
                        self.input_mode = InputMode::Normal;
                        self.input.clear();
                        self.history_index = None;
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
                                        self.add_to_log(format!(
                                            "{} Story name cannot be empty. Please try again:",
                                            Icons::cross()
                                        ));
                                        return None;
                                    }
                                    new_partial.name = Some(input);
                                    next_step = Some(StoryCreationStep::Header);
                                    self.add_to_log(format!("{} Story name saved", Icons::check()));
                                    self.add_to_log(format!(
                                        "{} Enter story header:",
                                        Icons::document()
                                    ));
                                }
                                StoryCreationStep::Header => {
                                    if input.is_empty() {
                                        self.add_to_log(format!(
                                            "{} Story header cannot be empty. Please try again:",
                                            Icons::cross()
                                        ));
                                        return None;
                                    }
                                    new_partial.header = Some(input);
                                    next_step = Some(StoryCreationStep::Body);
                                    self.add_to_log(format!(
                                        "{} Story header saved",
                                        Icons::check()
                                    ));
                                    self.add_to_log(format!("{} Enter story body:", Icons::book()));
                                }
                                StoryCreationStep::Body => {
                                    if input.is_empty() {
                                        self.add_to_log(format!(
                                            "{} Story body cannot be empty. Please try again:",
                                            Icons::cross()
                                        ));
                                        return None;
                                    }
                                    new_partial.body = Some(input);
                                    next_step = Some(StoryCreationStep::Channel);
                                    self.add_to_log(format!("{} Story body saved", Icons::check()));
                                    self.add_to_log(format!(
                                        "{} Enter channel (or press Enter for 'general'):",
                                        Icons::folder()
                                    ));
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
                                            format!("create s {name}|{header}|{body}|{ch}");
                                        self.input_mode = InputMode::Normal;
                                        self.add_to_log(format!(
                                            "{} Story creation complete!",
                                            Icons::check()
                                        ));
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
                InputMode::QuickReply { target_peer } => {
                    let target_peer = target_peer.clone(); // Clone to avoid borrow checker issues
                    match key.code {
                        KeyCode::Enter => {
                            let message = self.input.trim().to_string();
                            self.input.clear();
                            self.input_mode = InputMode::Normal;
                            if !message.is_empty() {
                                let cmd = format!("msg {} {}", target_peer, message);
                                self.add_to_log(format!("> {}", cmd));
                                return Some(AppEvent::Input(cmd));
                            }
                        }
                        KeyCode::Esc => {
                            self.input_mode = InputMode::Normal;
                            self.input.clear();
                            self.add_to_log(format!(
                                "{} Quick reply cancelled",
                                crate::types::Icons::cross()
                            ));
                        }
                        KeyCode::Char(c) => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                if c == 'c' {
                                    self.input_mode = InputMode::Normal;
                                    self.input.clear();
                                    self.add_to_log(format!(
                                        "{} Quick reply cancelled",
                                        crate::types::Icons::cross()
                                    ));
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
                InputMode::MessageComposition {
                    target_peer,
                    lines,
                    current_line,
                } => {
                    let target_peer = target_peer.clone(); // Clone to avoid borrow checker issues
                    let mut new_lines = lines.clone();
                    let mut new_current = current_line.clone();

                    match key.code {
                        KeyCode::Enter => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                // Ctrl+Enter to send the multi-line message
                                if !new_current.trim().is_empty() {
                                    new_lines.push(new_current.clone());
                                }
                                if !new_lines.is_empty() {
                                    let full_message = new_lines.join(" ");
                                    let cmd = format!("msg {} {}", target_peer, full_message);
                                    self.input.clear();
                                    self.input_mode = InputMode::Normal;
                                    self.add_to_log(format!("> {}", cmd));
                                    return Some(AppEvent::Input(cmd));
                                }
                            } else {
                                // Regular Enter adds a new line
                                new_lines.push(new_current.clone());
                                new_current.clear();
                                self.input.clear();
                                self.input_mode = InputMode::MessageComposition {
                                    target_peer: target_peer.clone(),
                                    lines: new_lines,
                                    current_line: new_current,
                                };
                            }
                        }
                        // Alternative way to send message with Ctrl+D (for terminals where Ctrl+Enter doesn't work)
                        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            // Ctrl+D to send the multi-line message (alternative to Ctrl+Enter)
                            if !new_current.trim().is_empty() {
                                new_lines.push(new_current.clone());
                            }
                            if !new_lines.is_empty() {
                                let full_message = new_lines.join(" ");
                                let cmd = format!("msg {} {}", target_peer, full_message);
                                self.input.clear();
                                self.input_mode = InputMode::Normal;
                                self.add_to_log(format!("> {}", cmd));
                                return Some(AppEvent::Input(cmd));
                            }
                        }
                        KeyCode::Esc => {
                            self.input_mode = InputMode::Normal;
                            self.input.clear();
                            self.add_to_log(format!(
                                "{} Message composition cancelled",
                                crate::types::Icons::cross()
                            ));
                        }
                        KeyCode::Char(c) => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                if c == 'c' {
                                    self.input_mode = InputMode::Normal;
                                    self.input.clear();
                                    self.add_to_log(format!(
                                        "{} Message composition cancelled",
                                        crate::types::Icons::cross()
                                    ));
                                }
                            } else {
                                new_current.push(c);
                                self.input = new_current.clone();
                                self.input_mode = InputMode::MessageComposition {
                                    target_peer: target_peer.clone(),
                                    lines: new_lines,
                                    current_line: new_current,
                                };
                            }
                        }
                        KeyCode::Backspace => {
                            if new_current.is_empty() && !new_lines.is_empty() {
                                // If current line is empty, go back to previous line
                                new_current = new_lines.pop().unwrap();
                                self.input = new_current.clone();
                                self.input_mode = InputMode::MessageComposition {
                                    target_peer: target_peer.clone(),
                                    lines: new_lines,
                                    current_line: new_current,
                                };
                            } else {
                                new_current.pop();
                                self.input = new_current.clone();
                                self.input_mode = InputMode::MessageComposition {
                                    target_peer: target_peer.clone(),
                                    lines: new_lines,
                                    current_line: new_current,
                                };
                            }
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
        self.add_to_log(format!("{} Output cleared", Icons::broom()));
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

    pub fn update_local_peer_id(&mut self, peer_id: String) {
        self.local_peer_id = Some(peer_id);
    }

    pub fn update_channels(&mut self, channels: Channels) {
        self.channels = channels;
        // Initialize selection to first channel if in channels view and we have channels
        if matches!(self.view_mode, ViewMode::Channels)
            && !self.channels.is_empty()
            && self.list_state.selected().is_none()
        {
            self.list_state.select(Some(0));
        }
    }

    pub fn update_unread_counts(&mut self, unread_counts: HashMap<String, usize>) {
        self.unread_counts = unread_counts;
    }

    pub fn update_network_health(
        &mut self,
        network_health: crate::network_circuit_breakers::NetworkHealthSummary,
    ) {
        self.network_health = Some(network_health);
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
            ViewMode::Stories(ref channel_name) => self
                .stories
                .iter()
                .filter(|story| story.channel == *channel_name)
                .count(),
            ViewMode::Conversations => self.conversation_manager.conversations.len(),
            ViewMode::ConversationThread(ref peer_id) => {
                // Number of messages in the active conversation
                self.conversation_manager
                    .get_conversation(peer_id)
                    .map(|c| c.messages.len())
                    .unwrap_or(0)
            }
        };

        if list_len > 0 {
            let current = self.list_state.selected().unwrap_or(0);
            let new_index = if current == 0 {
                list_len - 1
            } else {
                current - 1
            };
            self.list_state.select(Some(new_index));
        }
    }

    pub fn navigate_list_down(&mut self) {
        let list_len = match self.view_mode {
            ViewMode::Channels => self.channels.len(),
            ViewMode::Stories(ref channel_name) => self
                .stories
                .iter()
                .filter(|story| story.channel == *channel_name)
                .count(),
            ViewMode::Conversations => self.conversation_manager.conversations.len(),
            ViewMode::ConversationThread(ref peer_id) => {
                // Number of messages in the active conversation
                self.conversation_manager
                    .get_conversation(peer_id)
                    .map(|c| c.messages.len())
                    .unwrap_or(0)
            }
        };

        if list_len > 0 {
            let current = self.list_state.selected().unwrap_or(0);
            let new_index = if current >= list_len - 1 {
                0
            } else {
                current + 1
            };
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

    pub fn get_selected_story(&self) -> Option<Story> {
        // Check if we're in stories view and have stories available
        if let ViewMode::Stories(ref channel_name) = self.view_mode {
            let channel_stories: Vec<&Story> = self
                .stories
                .iter()
                .filter(|story| story.channel == *channel_name)
                .collect();

            if !channel_stories.is_empty() {
                if let Some(selected_index) = self.list_state.selected() {
                    if selected_index < channel_stories.len() {
                        Some(channel_stories[selected_index].clone())
                    } else {
                        // Fallback to first story if index is out of bounds
                        Some(channel_stories[0].clone())
                    }
                } else {
                    // No selection, default to first story
                    Some(channel_stories[0].clone())
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn display_story_content(&mut self, story: &Story) {
        // Sanitize all story content for safe display
        let sanitized_name = ContentSanitizer::sanitize_for_display(&story.name);
        let sanitized_header = ContentSanitizer::sanitize_for_display(&story.header);
        let sanitized_body = ContentSanitizer::sanitize_for_display(&story.body);
        let sanitized_channel = ContentSanitizer::sanitize_for_display(&story.channel);

        self.add_to_log("".to_string());
        self.add_to_log(format!(
            "{} ═══════════════ STORY CONTENT ═══════════════",
            Icons::book()
        ));
        self.add_to_log(format!("{} Title: {}", Icons::memo(), sanitized_name));
        self.add_to_log(format!("{}ID: {}", Icons::label(), story.id));
        self.add_to_log(format!(
            "{} Channel: {}",
            Icons::folder(),
            sanitized_channel
        ));
        self.add_to_log(format!(
            "{}Visibility: {}",
            Icons::eye(),
            if story.public { "Public" } else { "Private" }
        ));
        self.add_to_log(format!(
            "{} Created: {}",
            Icons::calendar(),
            format_timestamp(story.created_at)
        ));
        self.add_to_log("".to_string());
        self.add_to_log(format!("{} Header:", Icons::document()));
        self.add_to_log(format!("   {sanitized_header}"));
        self.add_to_log("".to_string());
        self.add_to_log(format!("{} Body:", Icons::book()));
        // Split the body into lines for better readability
        for line in sanitized_body.lines() {
            self.add_to_log(format!("   {line}"));
        }
        self.add_to_log(format!(
            "{} ═══════════════════════════════════════════",
            Icons::book()
        ));
        self.add_to_log("".to_string());
    }

    pub fn handle_direct_message(&mut self, dm: DirectMessage) {
        // Store the direct message in our dedicated storage (keep for backward compatibility)
        self.direct_messages.push(dm.clone());

        // Add message to conversation manager
        let local_peer_id = self.local_peer_id.as_deref().unwrap_or("unknown");
        self.conversation_manager
            .add_message(dm.clone(), local_peer_id);

        // Update last message sender for quick reply
        self.last_message_sender = Some(dm.from_name);

        // Update total unread count
        self.unread_message_count = self.conversation_manager.get_total_unread_count();
    }

    /// Try to auto-complete peer names for msg commands
    pub fn try_autocomplete_peer_name(&mut self) {
        let input_clone = self.input.clone(); // Clone to avoid borrow checker issues
        if let Some(partial_name) = input_clone.strip_prefix("msg ") {
            let partial_name = partial_name.trim();
            if partial_name.is_empty() {
                return;
            }

            // Find matching peer names
            let matches: Vec<String> = self
                .peers
                .values()
                .filter(|name| {
                    name.to_lowercase()
                        .starts_with(&partial_name.to_lowercase())
                })
                .cloned()
                .collect();

            match matches.len() {
                0 => {
                    // No matches
                    self.add_to_log(format!(
                        "{} No peers found matching '{}'",
                        crate::types::Icons::cross(),
                        partial_name
                    ));
                }
                1 => {
                    // Exact match, complete it
                    self.input = format!("msg {} ", matches[0]);
                    self.add_to_log(format!(
                        "{} Completed to '{}'",
                        crate::types::Icons::check(),
                        matches[0]
                    ));
                }
                _ => {
                    // Multiple matches, show them
                    self.add_to_log(format!("📋 Multiple matches: {}", matches.join(", ")));
                    // Find common prefix and complete up to that
                    if let Some(common_prefix) = find_common_prefix(&matches) {
                        if common_prefix.len() > partial_name.len() {
                            self.input = format!("msg {} ", common_prefix);
                        }
                    }
                }
            }
        }
    }

    pub fn mark_messages_as_read(&mut self) {
        // Reset unread count when user views messages
        self.unread_message_count = 0;
    }

    pub fn get_selected_conversation(&self) -> Option<&crate::types::Conversation> {
        if let Some(selected) = self.list_state.selected() {
            let conversations = self
                .conversation_manager
                .get_conversations_sorted_by_activity();
            conversations.get(selected).copied()
        } else {
            None
        }
    }

    pub fn enter_conversation_thread(&mut self, peer_id: String) {
        // Find peer name from the conversation
        if let Some(conversation) = self.conversation_manager.get_conversation(&peer_id) {
            self.view_mode = ViewMode::ConversationThread(conversation.peer_name.clone());
            self.conversation_manager
                .set_active_conversation(Some(peer_id));
            self.list_state.select(Some(0));
        }
    }

    pub fn return_to_conversations(&mut self) {
        self.view_mode = ViewMode::Conversations;
        self.conversation_manager.set_active_conversation(None);
        if !self.conversation_manager.conversations.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    pub fn update_conversation_manager(&mut self, conversation_manager: ConversationManager) {
        self.conversation_manager = conversation_manager;
        self.unread_message_count = self.conversation_manager.get_total_unread_count();
    }

    /// Display a user-friendly error message in the UI log
    pub fn display_error(&mut self, error: &crate::errors::AppError) {
        use crate::errors::{AppError, ConfigError, NetworkError, StorageError, UIError};

        let user_message = match error {
            AppError::Storage(StorageError::StoryNotFound { id }) => {
                format!("{} Story #{id} not found", Icons::cross())
            }
            AppError::Storage(StorageError::ChannelNotFound { name }) => {
                format!("{} Channel '{}' not found", Icons::cross(), name)
            }
            AppError::Storage(StorageError::DatabaseConnection { reason }) => {
                format!("{} Database connection failed: {}", Icons::cross(), reason)
            }
            AppError::Storage(StorageError::FileIO(_)) => {
                format!(
                    "{} File operation failed - check permissions",
                    Icons::cross()
                )
            }
            AppError::Network(NetworkError::SwarmCreation { reason }) => {
                format!("{} Network setup failed: {}", Icons::cross(), reason)
            }
            AppError::Network(NetworkError::PeerConnectionFailed { peer_id }) => {
                format!("{} Failed to connect to peer {}", Icons::cross(), peer_id)
            }
            AppError::Network(NetworkError::BroadcastFailed { reason }) => {
                format!("{} Message broadcast failed: {}", Icons::cross(), reason)
            }
            AppError::UI(UIError::TerminalInit(_)) => {
                format!(
                    "{} Terminal initialization failed - check terminal compatibility",
                    Icons::cross()
                )
            }
            AppError::Config(ConfigError::FileNotFound { path }) => {
                format!("{} Configuration file not found: {}", Icons::cross(), path)
            }
            AppError::Config(ConfigError::Validation { reason }) => {
                format!("{} Invalid configuration: {}", Icons::cross(), reason)
            }
            AppError::Crypto(crypto_error) => {
                format!("{} Encryption error: {}", Icons::cross(), crypto_error)
            }
            AppError::Relay(relay_error) => {
                format!("{} Relay error: {}", Icons::cross(), relay_error)
            }
            _ => {
                // Fallback for other errors
                format!("{} Error: {}", Icons::cross(), error)
            }
        };

        self.add_to_log(user_message);

        // Also log detailed error information for debugging
        debug!("Detailed error: {error:#}");
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
        self.add_to_log(format!(
            "{} Starting interactive story creation...",
            Icons::book()
        ));
        self.add_to_log(format!(
            "{} Enter story name (or Esc to cancel):",
            Icons::memo()
        ));
    }

    pub fn cancel_story_creation(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input.clear();
        self.add_to_log(format!("{} Story creation cancelled", Icons::cross()));
    }

    pub fn get_current_step_prompt(&self) -> String {
        match &self.input_mode {
            InputMode::CreatingStory { step, .. } => match step {
                StoryCreationStep::Name => format!("{} Enter story name:", Icons::memo()),
                StoryCreationStep::Header => format!("{} Enter story header:", Icons::document()),
                StoryCreationStep::Body => format!("{} Enter story body:", Icons::book()),
                StoryCreationStep::Channel => {
                    format!(
                        "{} Enter channel (or press Enter for 'general'):",
                        Icons::folder()
                    )
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

    pub fn draw(&mut self) -> UIResult<()> {
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
            // Add network health to status
            let network_health_text = if let Some(ref health) = self.network_health {
                if health.overall_healthy {
                    format!("{} Network OK", Icons::network_healthy())
                } else {
                    format!("{} Network Issues ({}/{})", Icons::network_issues(), health.failed_operations, health.total_operations)
                }
            } else {
                format!("{} Network Status Unknown", Icons::network_unknown())
            };
            let status_text = if let Some(ref name) = self.local_peer_name {
                format!(
                    "P2P-Play v{} | Peer: {} ({}) | Connected: {} | {} | Mode: {} | AUTO: {}",
                    version,
                    name,
                    self.local_peer_id.as_ref().map(|id| &id[..12]).unwrap_or("unknown"),
                    self.peers.len(),
                    network_health_text,
                    match self.input_mode {
                        InputMode::Normal => "Normal",
                        InputMode::Editing => "Editing",
                        InputMode::CreatingStory { .. } => "Creating Story",
                        InputMode::QuickReply { .. } => "Quick Reply",
                        InputMode::MessageComposition { .. } => "Message Composition",
                    },
                    if self.auto_scroll { "ON" } else { "OFF" }
                )
            } else {
                format!(
                    "P2P-Play v{} | Peer ID: {} | Connected: {} | {} | Mode: {} | AUTO: {}",
                    version,
                    self.local_peer_id.as_ref().map(|id| &id[..12]).unwrap_or("unknown"),
                    self.peers.len(),
                    network_health_text,
                    match self.input_mode {
                        InputMode::Normal => "Normal",
                        InputMode::Editing => "Editing",
                        InputMode::CreatingStory { .. } => "Creating Story",
                        InputMode::QuickReply { .. } => "Quick Reply",
                        InputMode::MessageComposition { .. } => "Message Composition",
                    },
                    if self.auto_scroll { "ON" } else { "OFF" }
                )
            };

            let status_bar_color = if let Some(ref health) = self.network_health {
                if health.overall_healthy {
                    Color::Green
                } else {
                    Color::Red
                }
            } else {
                Color::Yellow
            };

            let status_bar = Paragraph::new(status_text)
                .style(Style::default().fg(status_bar_color))
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

            // Side panels - split into three sections: peers, messages, and channels/stories
            let side_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(33), // Peers
                    Constraint::Percentage(33), // Direct Messages
                    Constraint::Percentage(34), // Stories/Channels
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
                        format!("{peer_id}")
                    } else if name.starts_with("Peer_") && name.contains(&peer_id.to_string()) {
                        // This is a default name we assigned (contains full peer ID), show truncated version
                        format!("Peer_{peer_id_display} [{peer_id_display}]")
                    } else {
                        // This is a real name the peer set (or a custom name that starts with "Peer_")
                        format!("{name} ({peer_id_display})")
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

            // Direct Messages panel
            let message_items: Vec<ListItem> = self
                .direct_messages
                .iter()
                .rev() // Show newest messages first
                .take(10) // Limit to last 10 messages for display
                .map(|dm| {
                    // Sanitize content for safe display
                    let sanitized_from_name = ContentSanitizer::sanitize_for_display(&dm.from_name);
                    let sanitized_message = ContentSanitizer::sanitize_for_display(&dm.message);

                    // Format timestamp as HH:MM in local timezone
                    let time_str = {
                        let datetime = UNIX_EPOCH + Duration::from_secs(dm.timestamp);
                        let local_datetime = DateTime::<Local>::from(datetime);
                        local_datetime.format("%H:%M").to_string()
                    };

                    // Truncate message if too long
                    let max_msg_len = 25; // Adjust based on panel width
                    let display_message = if sanitized_message.len() > max_msg_len {
                        format!("{}...", &sanitized_message[..max_msg_len])
                    } else {
                        sanitized_message
                    };

                    ListItem::new(format!(
                        "{} {} [{}]: {}",
                        Icons::envelope(),
                        sanitized_from_name,
                        time_str,
                        display_message
                    ))
                })
                .collect();

            // Create title with unread indicator
            let message_title = if self.unread_message_count > 0 {
                format!("Messages [{}]", self.unread_message_count)
            } else {
                "Messages".to_string()
            };

            let messages_list = List::new(message_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(message_title),
                )
                .highlight_style(Style::default().fg(Color::Yellow));
            f.render_widget(messages_list, side_chunks[1]);

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

                            // Get unread count for this channel
                            let unread_count = self.unread_counts.get(&channel.name).unwrap_or(&0);

                            // Create the display text with unread indicator
                            let display_text = if *unread_count > 0 {
                                format!("{} {} [{}] ({} stories) - {}",
                                    Icons::folder(), channel.name, unread_count, story_count, channel.description)
                            } else {
                                format!("{} {} ({} stories) - {}",
                                    Icons::folder(), channel.name, story_count, channel.description)
                            };

                            // Apply styling based on unread status
                            let item = ListItem::new(display_text);
                            if *unread_count > 0 {
                                // Highlight channels with unread stories in cyan to distinguish from selected items (yellow)
                                item.style(Style::default().fg(Color::Cyan))
                            } else {
                                item
                            }
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
                            let status = if story.public { Icons::book() } else { Icons::closed_book() };
                            ListItem::new(format!("{} {}: {}", status, story.id, story.name))
                        })
                        .collect();
                    (story_items, format!("Stories in '{selected_channel}' (Press Esc to return to channels)"))
                }
                ViewMode::Conversations => {
                    let conversation_items: Vec<ListItem> = self
                        .conversation_manager
                        .get_conversations_sorted_by_activity()
                        .iter()
                        .map(|conversation| {
                            let preview = conversation.get_last_message_preview();
                            let unread_indicator = if conversation.unread_count > 0 {
                                format!(" [{}]", conversation.unread_count)
                            } else {
                                String::new()
                            };

                            let display_text = format!("{} {}{} - {}",
                                Icons::memo(),
                                conversation.peer_name,
                                unread_indicator,
                                preview
                            );

                            let item = ListItem::new(display_text);
                            if conversation.unread_count > 0 {
                                item.style(Style::default().fg(Color::Cyan))
                            } else {
                                item
                            }
                        })
                        .collect();
                    (conversation_items, "Conversations (Press Enter to view thread)".to_string())
                }
                ViewMode::ConversationThread(peer_name) => {
                    let message_items: Vec<ListItem> = if let Some(conversation) = self.conversation_manager.get_active_conversation() {
                        conversation
                            .messages
                            .iter()
                            .map(|message| {
                                let timestamp = chrono::DateTime::from_timestamp(message.timestamp as i64, 0)
                                    .map(|dt| dt.format("%H:%M").to_string())
                                    .unwrap_or_else(|| "??:??".to_string());
                                ListItem::new(format!("[{}] {}: {}", timestamp, message.from_name, message.message))
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    (message_items, format!("Chat with '{}' (Press Esc to return to conversations)", peer_name))
                }
            };

            let list = List::new(list_items)
                .block(Block::default().borders(Borders::ALL).title(list_title))
                .highlight_style(Style::default().fg(Color::Yellow));
            f.render_stateful_widget(list, side_chunks[2], &mut self.list_state);

            // Input area
            let input_style = match self.input_mode {
                InputMode::Normal => Style::default(),
                InputMode::Editing => Style::default().fg(Color::Yellow),
                InputMode::CreatingStory { .. } => Style::default().fg(Color::Green),
                InputMode::QuickReply { .. } => Style::default().fg(Color::Cyan),
                InputMode::MessageComposition { .. } => Style::default().fg(Color::Magenta),
            };

            let input_text = match &self.input_mode {
                InputMode::Normal => {
                    match &self.view_mode {
                        ViewMode::Channels => "Press 'i' for input, 'r' reply, 'm' compose, Enter view, ←/→ nav, ↑/↓ scroll, Ctrl+S auto-scroll, 'c' clear, 'q' quit".to_string(),
                        ViewMode::Stories(_) => "Press 'i' for input, 'r' reply, 'm' compose, Enter view, Esc back, ←/→ nav, ↑/↓ scroll, Ctrl+S auto-scroll, 'c' clear, 'q' quit".to_string(),
                        ViewMode::Conversations => "Press 'i' for input, 'r' reply, 'm' compose, Enter view, ←/→ nav, ↑/↓ scroll, Tab conv, 'c' clear, 'q' quit".to_string(),
                        ViewMode::ConversationThread(_) => "Press 'i' for input, 'r' reply, 'm' compose, Esc back, ←/→ nav, ↑/↓ scroll, 'c' clear, 'q' quit".to_string(),
                    }
                }
                InputMode::Editing => format!("Command (Tab auto-complete, ↑/↓ history, Ctrl+L clear): {}", self.input),
                InputMode::QuickReply { target_peer } => format!("{} Quick reply to {}: {}", crate::types::Icons::envelope(), target_peer, self.input),
                InputMode::MessageComposition { target_peer, lines, current_line: _ } => {
                    if lines.is_empty() {
                        format!("{} Compose to {} (Enter new line, Ctrl+Enter/Ctrl+D send): {}", crate::types::Icons::memo(), target_peer, self.input)
                    } else {
                        format!("{} Compose to {} (Line {}, Ctrl+Enter/Ctrl+D send): {}", crate::types::Icons::memo(), target_peer, lines.len() + 1, self.input)
                    }
                }
                InputMode::CreatingStory { step, .. } => {
                    let prompt = match step {
                        StoryCreationStep::Name => format!("{} Story Name", Icons::memo()),
                        StoryCreationStep::Header => format!("{} Story Header", Icons::document()),
                        StoryCreationStep::Body => format!("{} Story Body", Icons::book()),
                        StoryCreationStep::Channel => format!("{} Channel (Enter for 'general')", Icons::folder()),
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
                    let prefix = "Command (Tab auto-complete, ↑/↓ history, Ctrl+L clear): ";
                    f.set_cursor(
                        chunks[2].x + 1 + prefix.chars().count() as u16 + self.input.chars().count() as u16,
                        chunks[2].y + 1,
                    );
                }
                InputMode::QuickReply { target_peer } => {
                    let prefix = format!("{} Quick reply to {}: ", crate::types::Icons::envelope(), target_peer);
                    f.set_cursor(
                        chunks[2].x + 1 + prefix.chars().count() as u16 + self.input.chars().count() as u16,
                        chunks[2].y + 1,
                    );
                }
                InputMode::MessageComposition { target_peer, lines, .. } => {
                    let prefix = if lines.is_empty() {
                        format!("{} Compose to {} (Enter new line, Ctrl+Enter/Ctrl+D send): ", crate::types::Icons::memo(), target_peer)
                    } else {
                        format!("{} Compose to {} (Line {}, Ctrl+Enter/Ctrl+D send): ", crate::types::Icons::memo(), target_peer, lines.len() + 1)
                    };
                    f.set_cursor(
                        chunks[2].x + 1 + prefix.chars().count() as u16 + self.input.chars().count() as u16,
                        chunks[2].y + 1,
                    );
                }
                InputMode::CreatingStory { step, .. } => {
                    let prefix = match step {
                        StoryCreationStep::Name => format!("{} Story Name: ", crate::types::Icons::memo()),
                        StoryCreationStep::Header => format!("{} Story Header: ", crate::types::Icons::document()),
                        StoryCreationStep::Body => format!("{} Story Body: ", crate::types::Icons::book()),
                        StoryCreationStep::Channel => format!("{} Channel (Enter for 'general'): ", crate::types::Icons::folder()),
                    };
                    f.set_cursor(
                        chunks[2].x + 1 + prefix.chars().count() as u16 + self.input.chars().count() as u16,
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
) -> UIResult<()> {
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

/// Helper function to format Unix timestamp to human-readable format
/// Find common prefix of a list of strings (case-insensitive)
fn find_common_prefix(strings: &[String]) -> Option<String> {
    if strings.is_empty() {
        return None;
    }

    if strings.len() == 1 {
        return Some(strings[0].clone());
    }

    let first = strings[0].to_lowercase();
    let mut prefix_len = 0;

    for i in 0..first.len() {
        let ch = first.chars().nth(i)?;

        for string in &strings[1..] {
            let other_ch = string.to_lowercase().chars().nth(i);
            if other_ch != Some(ch) {
                if prefix_len == 0 {
                    return None;
                } else {
                    return Some(strings[0][..prefix_len].to_string());
                }
            }
        }
        prefix_len = i + 1;
    }

    Some(strings[0][..prefix_len].to_string())
}

fn format_timestamp(timestamp: u64) -> String {
    use std::time::UNIX_EPOCH;

    if let Ok(duration) = UNIX_EPOCH.elapsed() {
        let current_timestamp = duration.as_secs();
        let diff = current_timestamp.saturating_sub(timestamp);

        if diff < 60 {
            "just now".to_string()
        } else if diff < 3600 {
            format!("{} minutes ago", diff / 60)
        } else if diff < 86400 {
            format!("{} hours ago", diff / 3600)
        } else {
            format!("{} days ago", diff / 86400)
        }
    } else {
        "unknown".to_string()
    }
}
