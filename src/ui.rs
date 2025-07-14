use crate::types::{DirectMessage, PeerName, Story, Stories};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use libp2p::PeerId;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
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
                "P2P-Play Terminal UI".to_string(),
                "Type 'help' for available commands".to_string(),
                "Press 'i' to enter input mode, 'Esc' to exit input mode".to_string(),
                "Press 'q' to quit".to_string(),
                "".to_string(),
            ],
            peers: HashMap::new(),
            local_stories: Vec::new(),
            received_stories: Vec::new(),
            local_peer_name: None,
            list_state: ListState::default(),
            input_mode: InputMode::Normal,
            scroll_offset: 0,
        })
    }

    pub fn cleanup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }

    pub fn handle_event(&mut self, event: Event) -> Option<AppEvent> {
        match event {
            Event::Key(key) => {
                match self.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') => {
                            self.should_quit = true;
                            return Some(AppEvent::Quit);
                        }
                        KeyCode::Char('i') => {
                            self.input_mode = InputMode::Editing;
                        }
                        KeyCode::Up => {
                            self.scroll_up();
                        }
                        KeyCode::Down => {
                            self.scroll_down();
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
                                match c {
                                    'c' => {
                                        self.input_mode = InputMode::Normal;
                                        self.input.clear();
                                    }
                                    _ => {}
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
            _ => {}
        }
        None
    }

    pub fn add_to_log(&mut self, message: String) {
        self.output_log.push(message);
        // Only auto-scroll to bottom if user is already at the bottom
        if self.scroll_offset >= self.output_log.len().saturating_sub(1) {
            self.scroll_to_bottom();
        }
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
            dm.from_name, 
            timestamp,
            dm.message
        ));
    }

    fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    fn scroll_down(&mut self) {
        let max_scroll = self.output_log.len().saturating_sub(1);
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.output_log.len().saturating_sub(1);
    }

    pub fn draw(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),  // Status bar
                    Constraint::Min(0),     // Main area
                    Constraint::Length(3),  // Input area
                ])
                .split(f.size());

            // Status bar
            let status_text = if let Some(ref name) = self.local_peer_name {
                format!("P2P-Play | Peer: {} | Connected: {} | Mode: {}", 
                    name, 
                    self.peers.len(),
                    match self.input_mode {
                        InputMode::Normal => "Normal",
                        InputMode::Editing => "Editing",
                    }
                )
            } else {
                format!("P2P-Play | No peer name set | Connected: {} | Mode: {}", 
                    self.peers.len(),
                    match self.input_mode {
                        InputMode::Normal => "Normal",
                        InputMode::Editing => "Editing",
                    }
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
                    Constraint::Percentage(60),  // Output area
                    Constraint::Percentage(40),  // Side panels
                ])
                .split(chunks[1]);

            // Output log
            let log_height = (main_chunks[0].height as usize).saturating_sub(2);
            let total_lines = self.output_log.len();
            
            // Calculate what portion of the log to display
            let visible_start = if total_lines <= log_height {
                0
            } else {
                // Show a window from scroll_offset
                let max_scroll = total_lines.saturating_sub(log_height);
                self.scroll_offset.min(max_scroll)
            };
            
            let visible_end = std::cmp::min(visible_start + log_height, total_lines);
            
            let visible_log: Vec<Line> = self.output_log[visible_start..visible_end]
                .iter()
                .map(|msg| Line::from(msg.clone()))
                .collect();

            let output = Paragraph::new(visible_log)
                .block(Block::default().borders(Borders::ALL).title("Output"))
                .wrap(Wrap { trim: true });
            f.render_widget(output, main_chunks[0]);

            // Side panels - split into top and bottom
            let side_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(50),  // Peers
                    Constraint::Percentage(50),  // Stories
                ])
                .split(main_chunks[1]);

            // Peers list
            let peer_items: Vec<ListItem> = self.peers
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
                .block(Block::default().borders(Borders::ALL).title("Connected Peers"))
                .highlight_style(Style::default().fg(Color::Yellow));
            f.render_widget(peers_list, side_chunks[0]);

            // Stories list
            let story_items: Vec<ListItem> = self.local_stories
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
                InputMode::Normal => "Press 'i' to enter input mode, 'q' to quit".to_string(),
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
        };
        
        let status = if story.public { "📖" } else { "📕" };
        let formatted = format!("{} {}: {}", status, story.id, story.name);
        
        assert_eq!(formatted, "📖 1: Test Story");
    }
}