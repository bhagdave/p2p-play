use crate::errors::UIResult;
use crate::types::{Channels, DirectMessage, Icons, Stories, Story};
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
    pub unread_counts: HashMap<String, usize>,
    pub view_mode: ViewMode,
    pub local_peer_name: Option<String>,
    pub local_peer_id: Option<String>,
    pub list_state: ListState,
    pub input_mode: InputMode,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub network_health: Option<crate::network_circuit_breakers::NetworkHealthSummary>,
    pub conversations: Vec<crate::types::Conversation>,
    pub unread_message_count: usize,                  
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub last_message_sender: Option<String>,
    pub notification_config: crate::types::MessageNotificationConfig,
    pub flash_active: bool,
    pub flash_start_time: Option<std::time::Instant>,
}

#[derive(PartialEq, Debug, Clone)]
pub enum ViewMode {
    Channels,
    Stories(String), // Selected channel name
    Conversations,
    ConversationView(String), // Selected peer_id
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
    ConversationViewed { peer_id: String },
}

impl App {
    pub fn new() -> UIResult<Self> {
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
            conversations: Vec::new(),
            unread_message_count: 0,
            input_history: Vec::new(),
            history_index: None,
            last_message_sender: None,
            notification_config: crate::types::MessageNotificationConfig::new(),
            flash_active: false,
            flash_start_time: None,
        })
    }

    pub fn cleanup(&mut self) -> UIResult<()> {
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
                    KeyCode::Left => match self.view_mode {
                        ViewMode::Channels => {
                            self.navigate_list_up();
                        }
                        ViewMode::Stories(_) => {
                            self.navigate_list_up();
                        }
                        ViewMode::Conversations => {
                            self.navigate_list_up();
                        }
                        ViewMode::ConversationView(_) => {}
                    },
                    KeyCode::Right => match self.view_mode {
                        ViewMode::Channels => {
                            self.navigate_list_down();
                        }
                        ViewMode::Stories(_) => {
                            self.navigate_list_down();
                        }
                        ViewMode::Conversations => {
                            self.navigate_list_down();
                        }
                        ViewMode::ConversationView(_) => {}
                    },
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
                        // Toggle auto-scroll on/off
                        self.auto_scroll = !self.auto_scroll;
                        if self.auto_scroll {
                            self.scroll_offset = 0;
                        }

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
                    KeyCode::Enter => match &self.view_mode {
                        ViewMode::Channels => {
                            if let Some(channel_name) = self.get_selected_channel() {
                                self.enter_channel(channel_name.to_string());
                            }
                        }
                        ViewMode::Stories(_) => {
                            if let Some(story) = self.get_selected_story() {
                                self.display_story_content(&story);
                                return Some(AppEvent::StoryViewed {
                                    story_id: story.id,
                                    channel: story.channel.clone(),
                                });
                            }
                        }
                        ViewMode::Conversations => {
                            if let Some(conversation) = self.get_selected_conversation() {
                                return Some(AppEvent::ConversationViewed {
                                    peer_id: conversation.peer_id.clone(),
                                });
                            }
                        }
                        ViewMode::ConversationView(_) => {}
                    },
                    KeyCode::Esc => match &self.view_mode {
                        ViewMode::Stories(_) => self.return_to_channels(),
                        ViewMode::ConversationView(_) => {
                            self.view_mode = ViewMode::Conversations;
                            self.list_state.select(Some(0));
                        }
                        _ => {}
                    },
                    KeyCode::Char('r') => {
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
                        self.input_mode = InputMode::Editing;
                        self.input = "compose ".to_string();
                    }
                    KeyCode::Char('M') => {
                        self.view_mode = ViewMode::Conversations;
                        self.list_state.select(Some(0));
                    }
                    KeyCode::Char('N') => {
                        self.view_mode = ViewMode::Channels;
                        self.list_state.select(Some(0));
                    }
                    _ => {}
                },
                InputMode::Editing => match key.code {
                    KeyCode::Enter => {
                        let input = self.input.clone();
                        self.input.clear();
                        self.input_mode = InputMode::Normal;
                        self.history_index = None;
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
                        if !self.input_history.is_empty() {
                            match self.history_index {
                                None => {
                                    self.history_index = Some(self.input_history.len() - 1);
                                    self.input =
                                        self.input_history[self.input_history.len() - 1].clone();
                                }
                                Some(idx) if idx > 0 => {
                                    self.history_index = Some(idx - 1);
                                    self.input = self.input_history[idx - 1].clone();
                                }
                                _ => {}
                            }
                        }
                    }
                    KeyCode::Down => {
                        if let Some(idx) = self.history_index {
                            if idx < self.input_history.len() - 1 {
                                self.history_index = Some(idx + 1);
                                self.input = self.input_history[idx + 1].clone();
                            } else {
                                self.history_index = None;
                                self.input.clear();
                            }
                        }
                    }
                    KeyCode::Tab => {
                        if self.input.starts_with("msg ") {
                            self.try_autocomplete_peer_name();
                        } else {
                            self.cycle_view_mode();
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
                            self.history_index = None;
                        }
                    }
                    KeyCode::Backspace => {
                        self.input.pop();
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
                    let target_peer = target_peer.clone();
                    let mut new_lines = lines.clone();
                    let mut new_current = current_line.clone();

                    match key.code {
                        KeyCode::Enter => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
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
                        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
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
        self.auto_scroll = true;
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
        self.list_state.select(Some(0));
    }

    pub fn return_to_channels(&mut self) {
        self.view_mode = ViewMode::Channels;
        if !self.channels.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    pub fn cycle_view_mode(&mut self) {
        self.view_mode = match &self.view_mode {
            ViewMode::Channels => ViewMode::Conversations,
            ViewMode::Conversations => ViewMode::Channels,
            ViewMode::Stories(_) => ViewMode::Channels,
            ViewMode::ConversationView(_) => ViewMode::Conversations,
        };
        self.list_state.select(Some(0));
    }

    pub fn navigate_list_up(&mut self) {
        let list_len = match self.view_mode {
            ViewMode::Channels => self.channels.len(),
            ViewMode::Stories(ref channel_name) => self
                .stories
                .iter()
                .filter(|story| story.channel == *channel_name)
                .count(),
            ViewMode::Conversations => self.conversations.len(),
            ViewMode::ConversationView(_) => 0,
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
            ViewMode::Conversations => self.conversations.len(),
            ViewMode::ConversationView(_) => 0,
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
        if matches!(self.view_mode, ViewMode::Channels) && !self.channels.is_empty() {
            if let Some(selected_index) = self.list_state.selected() {
                if selected_index < self.channels.len() {
                    Some(&self.channels[selected_index].name)
                } else {
                    Some(&self.channels[0].name)
                }
            } else {
                Some(&self.channels[0].name)
            }
        } else {
            None
        }
    }

    pub fn get_selected_story(&self) -> Option<Story> {
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
                        Some(channel_stories[0].clone())
                    }
                } else {
                    Some(channel_stories[0].clone())
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get_selected_conversation(&self) -> Option<&crate::types::Conversation> {
        if matches!(self.view_mode, ViewMode::Conversations) {
            if let Some(selected_index) = self.list_state.selected() {
                self.conversations.get(selected_index)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn display_story_content(&mut self, story: &Story) {
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
        self.last_message_sender = Some(dm.from_name.clone());

        self.trigger_message_notification(&dm);

        if let Ok(conversations) = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(crate::storage::get_conversations_with_status())
        }) {
            self.update_conversations(conversations);
        }
    }

    /// Trigger visual and audio notifications for new messages
    pub fn trigger_message_notification(&mut self, dm: &DirectMessage) {
        if self.notification_config.enable_flash_indicators && !dm.is_outgoing {
            self.flash_active = true;
            self.flash_start_time = Some(std::time::Instant::now());
        }

        if self.notification_config.enable_sound_notifications && !dm.is_outgoing {
            #[cfg(not(windows))]
            {
                // Simple bell sound for non-Windows systems
                let _ = std::process::Command::new("printf").arg("\x07").output();
            }
            #[cfg(windows)]
            {
                // Windows bell sound
                let _ = std::process::Command::new("cmd")
                    .args(&["/C", "echo", "\x07"])
                    .output();
            }
        }

        let notification_text = format!(
            "{} New message from {} {}",
            Icons::envelope(),
            dm.from_name,
            if self.notification_config.enable_flash_indicators {
                "📳"
            } else {
                ""
            }
        );
        self.add_to_log(notification_text);
    }

    pub fn update_flash_indicator(&mut self) {
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

    pub fn update_conversations(&mut self, conversations: Vec<crate::types::Conversation>) {
        self.unread_message_count = conversations.iter().map(|c| c.unread_count).sum();
        self.conversations = conversations;
    }

    pub async fn refresh_conversations(&mut self) {
        if let Ok(conversations) = crate::storage::get_conversations_with_status().await {
            self.update_conversations(conversations);
        }
    }

    pub async fn display_conversation(&mut self, peer_id: &str) {
        if let Ok(messages) = crate::storage::get_conversation_messages(peer_id).await {
            if let Some(conversation) = self
                .conversations
                .iter()
                .find(|c| c.peer_id == peer_id)
                .cloned()
            {
                self.add_to_log("".to_string());
                self.add_to_log(format!(
                    "{} ═══════════════ CONVERSATION WITH {} ═══════════════",
                    Icons::speech(),
                    conversation.peer_name
                ));
                self.add_to_log("".to_string());

                for msg in messages {
                    let time_str = {
                        let datetime = UNIX_EPOCH + Duration::from_secs(msg.timestamp);
                        let local_datetime = DateTime::<Local>::from(datetime);
                        if self.notification_config.enhanced_timestamps {
                            local_datetime.format("%Y-%m-%d %H:%M:%S").to_string()
                        } else {
                            local_datetime.format("%Y-%m-%d %H:%M").to_string()
                        }
                    };

                    let direction = if msg.is_outgoing { "→" } else { "←" };
                    let sanitized_message = ContentSanitizer::sanitize_for_display(&msg.message);

                    let status_indicator =
                        if msg.is_outgoing && self.notification_config.show_delivery_status {
                            format!(" {}", Icons::checkmark()) // Simple delivered indicator
                        } else {
                            String::new()
                        };

                    self.add_to_log(format!(
                        "{} [{}] {} {}{}",
                        direction,
                        time_str,
                        if msg.is_outgoing {
                            "You"
                        } else {
                            &conversation.peer_name
                        },
                        sanitized_message,
                        status_indicator
                    ));
                }
                self.add_to_log("".to_string());
                self.view_mode = ViewMode::ConversationView(peer_id.to_string());
            }
        }
    }

    pub fn try_autocomplete_peer_name(&mut self) {
        let input_clone = self.input.clone();
        if let Some(partial_name) = input_clone.strip_prefix("msg ") {
            let partial_name = partial_name.trim();
            if partial_name.is_empty() {
                return;
            }

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
                    self.add_to_log(format!("📋 Multiple matches: {}", matches.join(", ")));
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

    fn calculate_current_scroll_position(&self, available_height: usize) -> usize {
        let total_lines = self.output_log.len();

        if self.auto_scroll {
            if total_lines <= available_height {
                0
            } else {
                total_lines.saturating_sub(available_height)
            }
        } else {
            if total_lines <= available_height {
                0
            } else {
                let max_scroll = total_lines.saturating_sub(available_height);
                self.scroll_offset.min(max_scroll)
            }
        }
    }

    fn scroll_up(&mut self) {
        if self.auto_scroll {
            let estimated_height = 16;
            self.scroll_offset = self.calculate_current_scroll_position(estimated_height);
        }

        self.auto_scroll = false;

        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    fn scroll_down(&mut self) {
        if self.auto_scroll {
            let estimated_height = 16;
            self.scroll_offset = self.calculate_current_scroll_position(estimated_height);
        }

        self.auto_scroll = false;

        self.scroll_offset += 1;
    }

    pub fn draw(&mut self) -> UIResult<()> {
        self.update_flash_indicator();

        self.terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Status bar
                    Constraint::Min(0),    // Main area
                    Constraint::Length(3), // Input area
                ])
                .split(f.size());

            let version = env!("CARGO_PKG_VERSION");
            let network_health_text = if let Some(ref health) = self.network_health {
                if health.overall_healthy {
                    format!("{} Network OK", Icons::network_healthy())
                } else {
                    format!("{} Network Issues ({}/{})", Icons::network_issues(), health.failed_operations, health.total_operations)
                }
            } else {
                format!("{} Network Status Unknown", Icons::network_unknown())
            };

            let message_indicator = if self.unread_message_count > 0 {
                if self.flash_active {
                    format!(" | {} MSGS: {} 📳", Icons::envelope(), self.unread_message_count)
                } else {
                    format!(" | {} MSGS: {}", Icons::envelope(), self.unread_message_count)
                }
            } else {
                String::new()
            };

            let status_text = if let Some(ref name) = self.local_peer_name {
                format!(
                    "P2P-Play v{} | Peer: {} ({}) | Connected: {} | {} | Mode: {} | AUTO: {}{}",
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
                    if self.auto_scroll { "ON" } else { "OFF" },
                    message_indicator
                )
            } else {
                format!(
                    "P2P-Play v{} | Peer ID: {} | Connected: {} | {} | Mode: {} | AUTO: {}{}",
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
                    if self.auto_scroll { "ON" } else { "OFF" },
                    message_indicator
                )
            };

            let status_bar_color = if self.flash_active && self.notification_config.enable_flash_indicators {
                Color::LightYellow
            } else if let Some(ref health) = self.network_health {
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

            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Min(80), // Output area - minimum 80 characters
                    Constraint::Min(30), // Side panels - minimum 30 characters
                ])
                .split(chunks[1]);

            let actual_log_height = (main_chunks[0].height as usize).saturating_sub(2);
            let total_lines = self.output_log.len();

            let scroll_offset = if self.auto_scroll {
                if total_lines <= actual_log_height {
                    0
                } else {
                    total_lines.saturating_sub(actual_log_height)
                }
            } else {
                if total_lines <= actual_log_height {
                    0
                } else {
                    let max_scroll = total_lines.saturating_sub(actual_log_height);
                    self.scroll_offset.min(max_scroll)
                }
            };

            let visible_start = scroll_offset;
            let visible_end = std::cmp::min(visible_start + actual_log_height, total_lines);

            let lines: Vec<Line> = self.output_log[visible_start..visible_end]
                .iter()
                .map(|msg| Line::from(Span::raw(msg.clone())))
                .collect();

            let text = Text::from(lines);

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

            let side_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(33), // Peers
                    Constraint::Percentage(33), // Direct Messages
                    Constraint::Percentage(34), // Stories/Channels
                ])
                .split(main_chunks[1]);

            let peer_items: Vec<ListItem> = self
                .peers
                .iter()
                .map(|(peer_id, name)| {
                    let peer_id_str = peer_id.to_string();
                    let peer_id_display = if peer_id_str.len() >= 20 { &peer_id_str[..20] } else { &peer_id_str };

                    let content = if name.is_empty() {
                        format!("{peer_id}")
                    } else if name.starts_with("Peer_") && name.contains(&peer_id.to_string()) {
                        format!("Peer_{peer_id_display} [{peer_id_display}]")
                    } else {
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

            let conversation_items: Vec<ListItem> = self
                .conversations
                .iter()
                .map(|conv| {
                    let sanitized_peer_name = ContentSanitizer::sanitize_for_display(&conv.peer_name);

                    let time_str = if conv.last_activity > 0 {
                        let datetime = UNIX_EPOCH + Duration::from_secs(conv.last_activity);
                        let local_datetime = DateTime::<Local>::from(datetime);
                        if self.notification_config.enhanced_timestamps {
                            let now = std::time::SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let age_secs = now.saturating_sub(conv.last_activity);

                            if age_secs < 60 {
                                "now".to_string()
                            } else if age_secs < 3600 {
                                format!("{}m", age_secs / 60)
                            } else if age_secs < 86400 {
                                local_datetime.format("%H:%M").to_string()
                            } else if age_secs < 604800 {
                                local_datetime.format("%a %H:%M").to_string()
                            } else {
                                local_datetime.format("%m/%d").to_string()
                            }
                        } else {
                            local_datetime.format("%H:%M").to_string()
                        }
                    } else {
                        "--:--".to_string()
                    };

                    let unread_indicator = if conv.unread_count > 0 {
                        format!(" ({})", conv.unread_count)
                    } else {
                        String::new()
                    };

                    let item_text = format!(
                        "{} {} [{}]{}",
                        Icons::speech(),
                        sanitized_peer_name,
                        time_str,
                        unread_indicator
                    );

                    if self.notification_config.enable_color_coding && conv.unread_count > 0 {
                        ListItem::new(item_text).style(Style::default().fg(Color::Magenta))
                    } else {
                        ListItem::new(item_text)
                    }
                })
                .collect();

            let message_title = if self.unread_message_count > 0 {
                format!("Conversations [{}]", self.unread_message_count)
            } else {
                "Conversations".to_string()
            };

            let conversations_list = List::new(conversation_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(message_title)
                        .border_style(if matches!(self.view_mode, ViewMode::Conversations) {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        }),
                )
                .highlight_style(Style::default().fg(Color::Yellow));
            if matches!(self.view_mode, ViewMode::Conversations) {
                f.render_stateful_widget(conversations_list, side_chunks[1], &mut self.list_state);
            } else {
                f.render_widget(conversations_list, side_chunks[1]);
            }

            let (list_items, list_title) = match &self.view_mode {
                ViewMode::Channels => {
                    let channel_items: Vec<ListItem> = self
                        .channels
                        .iter()
                        .map(|channel| {
                            let story_count = self.stories.iter()
                                .filter(|story| story.channel == channel.name)
                                .count();

                            let unread_count = self.unread_counts.get(&channel.name).unwrap_or(&0);

                            let display_text = if *unread_count > 0 {
                                format!("{} {} [{}] ({} stories) - {}",
                                    Icons::folder(), channel.name, unread_count, story_count, channel.description)
                            } else {
                                format!("{} {} ({} stories) - {}",
                                    Icons::folder(), channel.name, story_count, channel.description)
                            };

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
                    (Vec::new(), "Use Tab to focus on Conversations panel".to_string())
                }
                ViewMode::ConversationView(_) => {
                    (Vec::new(), "Viewing conversation (Press Esc to return)".to_string())
                }
            };

            let list = List::new(list_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(list_title)
                        .border_style(if matches!(self.view_mode, ViewMode::Channels | ViewMode::Stories(_)) {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        })
                )
                .highlight_style(Style::default().fg(Color::Yellow));

            if matches!(self.view_mode, ViewMode::Channels | ViewMode::Stories(_)) {
                f.render_stateful_widget(list, side_chunks[2], &mut self.list_state);
            } else {
                f.render_widget(list, side_chunks[2]);
            }

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
                        ViewMode::Channels => "Press 'i' for input, 'r' reply, 'm' compose, 'M' messages, Enter view, ←/→ nav, ↑/↓ scroll, Ctrl+S auto-scroll, 'c' clear, 'q' quit".to_string(),
                        ViewMode::Stories(_) => "Press 'i' for input, 'r' reply, 'm' compose, 'M' messages, Enter view, Esc back, ←/→ nav, ↑/↓ scroll, Ctrl+S auto-scroll, 'c' clear, 'q' quit".to_string(),
                        ViewMode::Conversations => "Press 'i' for input, 'r' reply, 'm' compose, 'N' channels, Enter view, ←/→ nav, ↑/↓ scroll, Ctrl+S auto-scroll, 'c' clear, 'q' quit".to_string(),
                        ViewMode::ConversationView(_) => "Press 'i' for input, 'r' reply, 'm' compose, Esc back, 'c' clear, 'q' quit".to_string(),
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
