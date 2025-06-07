use crate::types::{Stories, Story};
use log::{error, info};
use std::error::Error;
use tokio::fs;

const STORAGE_FILE_PATH: &str = "./stories.json";

pub async fn read_local_stories() -> Result<Stories, Box<dyn Error>> {
    let content = fs::read(STORAGE_FILE_PATH).await?;
    let result = serde_json::from_slice(&content)?;
    Ok(result)
}

pub async fn write_local_stories(stories: &Stories) -> Result<(), Box<dyn Error>> {
    let json = serde_json::to_string(&stories)?;
    fs::write(STORAGE_FILE_PATH, &json).await?;
    Ok(())
}

pub async fn create_new_story(name: &str, header: &str, body: &str) -> Result<(), Box<dyn Error>> {
    let mut local_stories = read_local_stories().await?;
    let new_id = match local_stories.iter().max_by_key(|r| r.id) {
        Some(v) => v.id + 1,
        None => 0,
    };
    local_stories.push(Story {
        id: new_id,
        name: name.to_owned(),
        header: header.to_owned(),
        body: body.to_owned(),
        public: false,
    });
    write_local_stories(&local_stories).await?;

    info!("Created story:");
    info!("Name: {}", name);
    info!("Header: {}", header);
    info!("Body: {}", body);

    Ok(())
}

pub async fn publish_story(id: usize, sender: tokio::sync::mpsc::UnboundedSender<Story>) -> Result<(), Box<dyn Error>> {
    let mut local_stories = read_local_stories().await?;
    let mut published_story = None;
    
    for story in local_stories.iter_mut() {
        if story.id == id {
            story.public = true;
            published_story = Some(story.clone());
            break;
        }
    }
    
    write_local_stories(&local_stories).await?;
    
    if let Some(story) = published_story {
        if let Err(e) = sender.send(story) {
            error!("error sending story for broadcast: {}", e);
        }
    }
    
    Ok(())
}

pub async fn save_received_story(mut story: Story) -> Result<(), Box<dyn Error>> {
    let mut local_stories = match read_local_stories().await {
        Ok(stories) => stories,
        Err(_) => Vec::new(), // Create empty vec if no file exists
    };
    
    // Check if story already exists (by name and content to avoid duplicates)
    let already_exists = local_stories.iter().any(|s| 
        s.name == story.name && s.header == story.header && s.body == story.body
    );
    
    if !already_exists {
        // Assign new local ID
        let new_id = match local_stories.iter().max_by_key(|r| r.id) {
            Some(v) => v.id + 1,
            None => 0,
        };
        story.id = new_id;
        story.public = true; // Mark as public since it was published
        
        local_stories.push(story);
        write_local_stories(&local_stories).await?;
        info!("Saved received story to local storage with ID: {}", new_id);
    } else {
        info!("Story already exists locally, skipping save");
    }
    
    Ok(())
}