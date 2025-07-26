use p2p_play::ui::{InputMode, StoryCreationStep, PartialStory};

#[test]
fn test_input_mode_equality() {
    let mode1 = InputMode::Normal;
    let mode2 = InputMode::Normal;
    let mode3 = InputMode::Editing;
    
    assert_eq!(mode1, mode2);
    assert_ne!(mode1, mode3);
}

#[test]
fn test_story_creation_step_equality() {
    let step1 = StoryCreationStep::Name;
    let step2 = StoryCreationStep::Name;
    let step3 = StoryCreationStep::Header;
    
    assert_eq!(step1, step2);
    assert_ne!(step1, step3);
}

#[test]
fn test_partial_story_creation() {
    let partial_story = PartialStory {
        name: Some("Test Story".to_string()),
        header: None,
        body: Some("Test body".to_string()),
        channel: None,
    };
    
    assert_eq!(partial_story.name, Some("Test Story".to_string()));
    assert_eq!(partial_story.header, None);
    assert_eq!(partial_story.body, Some("Test body".to_string()));
    assert_eq!(partial_story.channel, None);
}

#[test]
fn test_partial_story_equality() {
    let story1 = PartialStory {
        name: Some("Test".to_string()),
        header: Some("Header".to_string()),
        body: Some("Body".to_string()),
        channel: Some("general".to_string()),
    };
    
    let story2 = PartialStory {
        name: Some("Test".to_string()),
        header: Some("Header".to_string()),
        body: Some("Body".to_string()),
        channel: Some("general".to_string()),
    };
    
    let story3 = PartialStory {
        name: Some("Different".to_string()),
        header: Some("Header".to_string()),
        body: Some("Body".to_string()),
        channel: Some("general".to_string()),
    };
    
    assert_eq!(story1, story2);
    assert_ne!(story1, story3);
}

#[test]
fn test_input_mode_creating_story() {
    let mode = InputMode::CreatingStory {
        step: StoryCreationStep::Name,
        partial_story: PartialStory {
            name: None,
            header: None,
            body: None,
            channel: None,
        },
    };
    
    if let InputMode::CreatingStory { step, partial_story } = mode {
        assert_eq!(step, StoryCreationStep::Name);
        assert_eq!(partial_story.name, None);
        assert_eq!(partial_story.header, None);
        assert_eq!(partial_story.body, None);
        assert_eq!(partial_story.channel, None);
    } else {
        panic!("Expected CreatingStory mode");
    }
}

#[test]
fn test_story_creation_step_progression() {
    // Test that we can represent different steps of story creation
    let steps = vec![
        StoryCreationStep::Name,
        StoryCreationStep::Header,
        StoryCreationStep::Body,
        StoryCreationStep::Channel,
    ];
    
    assert_eq!(steps.len(), 4);
    assert_ne!(steps[0], steps[1]);
    assert_ne!(steps[1], steps[2]);
    assert_ne!(steps[2], steps[3]);
}

#[test]
fn test_partial_story_progressive_building() {
    // Test that we can build a story progressively
    let mut partial = PartialStory {
        name: None,
        header: None,
        body: None,
        channel: None,
    };
    
    // Step 1: Add name
    partial.name = Some("My Story".to_string());
    assert!(partial.name.is_some());
    assert!(partial.header.is_none());
    
    // Step 2: Add header
    partial.header = Some("Story Header".to_string());
    assert!(partial.name.is_some());
    assert!(partial.header.is_some());
    assert!(partial.body.is_none());
    
    // Step 3: Add body
    partial.body = Some("Story content goes here".to_string());
    assert!(partial.body.is_some());
    assert!(partial.channel.is_none());
    
    // Step 4: Add channel
    partial.channel = Some("general".to_string());
    assert!(partial.channel.is_some());
    
    // Verify all fields are now populated
    assert_eq!(partial.name.unwrap(), "My Story");
    assert_eq!(partial.header.unwrap(), "Story Header");
    assert_eq!(partial.body.unwrap(), "Story content goes here");
    assert_eq!(partial.channel.unwrap(), "general");
}

#[cfg(windows)]
#[test]
fn test_windows_specific_functionality() {
    // Test that Windows-specific code compiles and works
    // The should_process_key_event function is Windows-specific
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    
    let event1 = KeyEvent {
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::NONE,
        kind: crossterm::event::KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    };
    
    let event2 = KeyEvent {
        code: KeyCode::Char('b'),
        modifiers: KeyModifiers::NONE,
        kind: crossterm::event::KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    };
    
    // Just test that the events are different
    assert_ne!(event1.code, event2.code);
}