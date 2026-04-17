//! Story command handlers: create, list, publish, show, delete, search, filter, export.

use crate::error_logger::ErrorLogger;
use crate::storage::{
    create_new_story_with_channel, delete_local_story, filter_stories_by_channel,
    filter_stories_by_recent_days, mark_story_as_read, publish_story, read_local_stories,
    search_stories,
};
use crate::types::{ActionResult, Icons, ListMode, ListRequest, Story};
use crate::validation::ContentValidator;
use bytes::Bytes;
use libp2p::swarm::Swarm;

use super::{UILogger, validate_and_log};
use crate::network::{StoryBehaviour, TOPIC};

// ---------------------------------------------------------------------------
// Shared formatting helpers
// ---------------------------------------------------------------------------

/// Returns the public/private icon + label string for a story.
pub(super) fn format_story_status(public: bool) -> String {
    if public {
        format!("{} Public", Icons::book())
    } else {
        format!("{} Private", Icons::closed_book())
    }
}

/// One-line story summary: `<status> | Channel: <ch> | <id>: <name>`.
pub(super) fn format_story_line(story: &Story) -> String {
    format!(
        "{} | Channel: {} | {}: {}",
        format_story_status(story.public),
        story.channel,
        story.id,
        story.name
    )
}

/// One-line story summary without channel: `<status> | <id>: <name>`.
pub(super) fn format_story_line_no_channel(story: &Story) -> String {
    format!(
        "{} | {}: {}",
        format_story_status(story.public),
        story.id,
        story.name
    )
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn handle_list_stories(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) {
    let rest = cmd.strip_prefix("ls s ");
    match rest {
        Some("all") => {
            ui_logger.log("Requesting all stories from all peers".to_string());
            let req = ListRequest {
                mode: ListMode::ALL,
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            let json_bytes = Bytes::from(json.into_bytes());
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json_bytes);
        }
        Some(story_peer_id) => {
            ui_logger.log(format!("Requesting all stories from peer: {story_peer_id}"));
            let req = ListRequest {
                mode: ListMode::One(story_peer_id.to_owned()),
            };
            let json = serde_json::to_string(&req).expect("can jsonify request");
            let json_bytes = Bytes::from(json.into_bytes());
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json_bytes);
        }
        None => {
            ui_logger.log("Local stories:".to_string());
            match read_local_stories().await {
                Ok(v) => {
                    ui_logger.log(format!("Local stories ({})", v.len()));
                    v.iter().for_each(|r| {
                        ui_logger.log(format_story_line(r));
                    });
                }
                Err(e) => error_logger.log_error(&format!("Failed to fetch local stories: {e}")),
            };
        }
    };
}

pub async fn handle_create_stories_with_sender(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
    story_sender: Option<tokio::sync::mpsc::UnboundedSender<crate::types::Story>>,
) -> Option<ActionResult> {
    if let Some(rest) = cmd.strip_prefix("create s") {
        let rest = rest.trim();

        if rest.is_empty() {
            ui_logger.log(format!(
                "{} Starting interactive story creation...",
                Icons::book()
            ));
            ui_logger.log(format!(
                "{} This will guide you through creating a story step by step.",
                Icons::memo()
            ));
            ui_logger.log(format!("{} Use Esc at any time to cancel.", Icons::pin()));
            return Some(ActionResult::StartStoryCreation);
        } else {
            let elements: Vec<&str> = rest.split('|').collect();
            if elements.len() < 3 {
                ui_logger.log(
                    "too few arguments - Format: name|header|body or name|header|body|channel"
                        .to_string(),
                );
            } else {
                let name = elements.first().expect("name is there");
                let header = elements.get(1).expect("header is there");
                let body = elements.get(2).expect("body is there");
                let channel = elements.get(3).unwrap_or(&"general");

                let validated_name = validate_and_log(
                    ContentValidator::validate_story_name(name),
                    "story name",
                    ui_logger,
                )?;
                let validated_header = validate_and_log(
                    ContentValidator::validate_story_header(header),
                    "story header",
                    ui_logger,
                )?;
                let validated_body = validate_and_log(
                    ContentValidator::validate_story_body(body),
                    "story body",
                    ui_logger,
                )?;
                let validated_channel = validate_and_log(
                    ContentValidator::validate_channel_name(channel),
                    "channel name",
                    ui_logger,
                )?;

                if let Err(e) = create_new_story_with_channel(
                    &validated_name,
                    &validated_header,
                    &validated_body,
                    &validated_channel,
                )
                .await
                {
                    error_logger.log_error(&format!("Failed to create story: {e}"));
                } else {
                    ui_logger.log(format!(
                        "Story created and auto-published to channel '{validated_channel}'"
                    ));

                    if let Some(sender) = story_sender {
                        match read_local_stories().await {
                            Ok(stories) => {
                                if let Some(created_story) = stories.iter().find(|s| {
                                    s.name == validated_name
                                        && s.header == validated_header
                                        && s.body == validated_body
                                }) {
                                    if let Err(e) = sender.send(created_story.clone()) {
                                        error_logger.log_error(&format!(
                                            "Failed to broadcast newly created story: {e}"
                                        ));
                                    } else {
                                        ui_logger.log(
                                            "Story automatically shared with connected peers"
                                                .to_string(),
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                error_logger.log_error(&format!(
                                    "Failed to read stories for auto-broadcast: {e}"
                                ));
                            }
                        }
                    }

                    return Some(ActionResult::RefreshStories);
                };
            }
        }
    }
    None
}

pub async fn handle_publish_story(
    cmd: &str,
    story_sender: tokio::sync::mpsc::UnboundedSender<Story>,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) {
    if let Some(rest) = cmd.strip_prefix("publish s") {
        match ContentValidator::validate_story_id(rest) {
            Ok(id) => {
                if let Err(e) = publish_story(id, story_sender).await {
                    error_logger.log_error(&format!("Failed to publish story with id {id}: {e}"));
                } else {
                    ui_logger.log(format!("Published story with id: {id}"));
                }
            }
            Err(e) => ui_logger.log(format!("Invalid story ID: {e}")),
        };
    }
}

pub async fn handle_show_story(cmd: &str, ui_logger: &UILogger, peer_id: &str) {
    if let Some(rest) = cmd.strip_prefix("show story ") {
        match ContentValidator::validate_story_id(rest) {
            Ok(id) => match read_local_stories().await {
                Ok(stories) => {
                    if let Some(story) = stories.iter().find(|s| s.id == id) {
                        ui_logger.log(format!(
                            "{} Story {}: {}",
                            Icons::book(),
                            story.id,
                            story.name
                        ));
                        ui_logger.log(format!("Channel: {}", story.channel));
                        ui_logger.log(format!("Header: {}", story.header));
                        ui_logger.log(format!("Body: {}", story.body));
                        ui_logger.log(format!(
                            "Public: {}",
                            if story.public { "Yes" } else { "No" }
                        ));

                        let _ = mark_story_as_read(story.id, peer_id, &story.channel).await;
                    } else {
                        ui_logger.log(format!("Story with id {id} not found"));
                    }
                }
                Err(e) => {
                    ui_logger.log(format!("Error reading stories: {e}"));
                }
            },
            Err(e) => {
                ui_logger.log(format!("Invalid story ID: {e}"));
            }
        }
    } else {
        ui_logger.log("Usage: show story <id>".to_string());
    }
}

pub async fn handle_delete_story(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<ActionResult> {
    if let Some(rest) = cmd.strip_prefix("delete s ") {
        let id_strings: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
        let mut successful_deletions = 0;
        let mut failed_deletions = Vec::new();

        let valid_id_strings: Vec<&str> =
            id_strings.into_iter().filter(|s| !s.is_empty()).collect();

        if valid_id_strings.is_empty() {
            ui_logger.log("Usage: delete s <id1>[,<id2>,<id3>...]".to_string());
            return None;
        }

        let is_batch_operation = valid_id_strings.len() > 1;

        for id_str in valid_id_strings {
            match ContentValidator::validate_story_id(id_str) {
                Ok(id) => match delete_local_story(id).await {
                    Ok(deleted) => {
                        if deleted {
                            ui_logger.log(format!("Story {id} deleted successfully"));
                            successful_deletions += 1;
                        } else {
                            let failure_msg = format!("Story {id} not found");
                            ui_logger.log(failure_msg.to_string());
                            failed_deletions.push(failure_msg);
                        }
                    }
                    Err(e) => {
                        let failure_msg = format!("Failed to delete story {id}: {e}");
                        ui_logger.log(failure_msg.to_string());
                        error_logger.log_error(&failure_msg);
                        failed_deletions.push(failure_msg);
                    }
                },
                Err(e) => {
                    let failure_msg = format!("Invalid story ID: {e}");
                    ui_logger.log(failure_msg.to_string());
                    failed_deletions.push(failure_msg);
                }
            }
        }

        if is_batch_operation && !failed_deletions.is_empty() {
            let total_failed = failed_deletions.len();
            use crate::errors::StorageError;
            let batch_error = StorageError::batch_operation_failed(
                successful_deletions,
                total_failed,
                failed_deletions,
            );
            ui_logger.log(format!("Batch deletion summary: {batch_error}"));
            error_logger.log_error(&format!(
                "Batch delete operation completed with errors: {batch_error}"
            ));
        }

        if successful_deletions > 0 {
            return Some(ActionResult::RefreshStories);
        }
    } else {
        ui_logger.log("Usage: delete s <id1>[,<id2>,<id3>...]".to_string());
    }
    None
}

pub async fn handle_search_stories(cmd: &str, ui_logger: &UILogger, error_logger: &ErrorLogger) {
    if let Some(rest) = cmd.strip_prefix("search ") {
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.is_empty() {
            ui_logger.log("Usage: search <query> [channel:<channel>] [author:<peer>] [recent:<days>] [public|private]".to_string());
            return;
        }

        let mut query = crate::types::SearchQuery::new(String::new());
        let mut search_terms = Vec::new();

        for part in parts {
            if let Some(channel) = part.strip_prefix("channel:") {
                query = query.with_channel(channel.to_string());
            } else if part.starts_with("author:") {
                ui_logger.log("Author filtering is not yet implemented".to_string());
                return;
            } else if let Some(days_str) = part.strip_prefix("recent:") {
                if let Ok(days) = days_str.parse::<u32>() {
                    query = query.with_date_range_days(days);
                } else {
                    ui_logger.log(format!("Invalid number of days: '{days_str}'"));
                    return;
                }
            } else if part == "public" {
                query = query.with_visibility_filter(true);
            } else if part == "private" {
                query = query.with_visibility_filter(false);
            } else {
                search_terms.push(part);
            }
        }

        query.text = search_terms.join(" ");

        if query.is_empty() {
            ui_logger.log("Please provide a search query or filter criteria".to_string());
            return;
        }

        match search_stories(&query).await {
            Ok(results) => {
                if results.is_empty() {
                    ui_logger.log("No stories found matching your search criteria".to_string());
                } else {
                    ui_logger.log(format!("Found {} matching stories:", results.len()));
                    for result in results {
                        let story = &result.story;
                        let relevance = if let Some(score) = result.relevance_score {
                            format!(" (relevance: {score:.1})")
                        } else {
                            String::new()
                        };
                        ui_logger.log(format!(
                            "{}{}",
                            format_story_line(story),
                            relevance
                        ));
                    }
                }
            }
            Err(e) => {
                error_logger.log_error(&format!("Search failed: {e}"));
                ui_logger.log("Search failed - check error logs for details".to_string());
            }
        }
    } else {
        ui_logger.log("Usage: search <query> [channel:<channel>] [author:<peer>] [recent:<days>] [public|private]".to_string());
    }
}

pub async fn handle_filter_stories(cmd: &str, ui_logger: &UILogger, error_logger: &ErrorLogger) {
    if let Some(rest) = cmd.strip_prefix("filter ") {
        if let Some(channel) = rest.strip_prefix("channel ") {
            let channel_name = channel.trim();
            if channel_name.is_empty() {
                ui_logger.log("Usage: filter channel <channel_name>".to_string());
                return;
            }

            match filter_stories_by_channel(channel_name).await {
                Ok(stories) => {
                    if stories.is_empty() {
                        ui_logger.log(format!("No stories found in channel '{channel_name}'"));
                    } else {
                        ui_logger.log(format!(
                            "Stories in channel '{}' ({}):",
                            channel_name,
                            stories.len()
                        ));
                        for story in stories {
                            ui_logger.log(format_story_line_no_channel(&story));
                        }
                    }
                }
                Err(e) => {
                    error_logger.log_error(&format!("Filter failed: {e}"));
                    ui_logger.log("Filter failed - check error logs for details".to_string());
                }
            }
        } else if let Some(days_str) = rest.strip_prefix("recent ") {
            match days_str.trim().parse::<u32>() {
                Ok(days) => match filter_stories_by_recent_days(days).await {
                    Ok(stories) => {
                        if stories.is_empty() {
                            ui_logger.log(format!("No stories found from the last {days} days"));
                        } else {
                            ui_logger.log(format!(
                                "Stories from the last {} days ({}):",
                                days,
                                stories.len()
                            ));
                            for story in stories {
                                ui_logger.log(format_story_line(&story));
                            }
                        }
                    }
                    Err(e) => {
                        error_logger.log_error(&format!("Recent filter failed: {e}"));
                        ui_logger
                            .log("Recent filter failed - check error logs for details".to_string());
                    }
                },
                Err(_) => {
                    ui_logger.log(format!("Invalid number of days: '{days_str}'"));
                    ui_logger.log("Usage: filter recent <days>".to_string());
                }
            }
        } else {
            ui_logger
                .log("Usage: filter channel <channel_name> | filter recent <days>".to_string());
        }
    } else {
        ui_logger.log("Usage: filter channel <channel_name> | filter recent <days>".to_string());
    }
}

pub async fn handle_export_story(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
    export_dir: &std::path::Path,
) {
    let rest = match cmd.strip_prefix("export s ") {
        Some(r) => r.trim(),
        None => {
            ui_logger
                .log("Usage: export s <id|all> <md|json>  (exports to ./exports/)".to_string());
            return;
        }
    };

    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
    if parts.len() != 2 {
        ui_logger.log("Usage: export s <id|all> <md|json>".to_string());
        return;
    }

    let target = parts[0].trim();
    let format = parts[1].trim().to_lowercase();

    if format != "md" && format != "json" {
        ui_logger.log("Format must be 'md' or 'json'".to_string());
        return;
    }

    let stories = match read_local_stories().await {
        Ok(s) => s,
        Err(e) => {
            error_logger.log_error(&format!("Failed to read stories for export: {e}"));
            ui_logger.log(format!("{} Failed to read stories: {e}", Icons::cross()));
            return;
        }
    };

    let export_all = target == "all";
    let to_export: Vec<&crate::types::Story> = if export_all {
        stories.iter().collect()
    } else {
        match ContentValidator::validate_story_id(target) {
            Ok(id) => match stories.iter().find(|s| s.id == id) {
                Some(s) => vec![s],
                None => {
                    ui_logger.log(format!("Story with id {id} not found"));
                    return;
                }
            },
            Err(e) => {
                ui_logger.log(format!("Invalid story ID: {e}"));
                return;
            }
        }
    };

    if let Err(e) = tokio::fs::create_dir_all(export_dir).await {
        error_logger.log_error(&format!("Failed to create exports directory: {e}"));
        ui_logger.log(format!(
            "{} Failed to create exports directory: {e}",
            Icons::cross()
        ));
        return;
    }

    if to_export.is_empty() {
        ui_logger.log("No stories to export".to_string());
        return;
    }

    match format.as_str() {
        "json" => {
            let path = if export_all {
                export_dir.join("stories.json")
            } else {
                export_dir.join(format!("story_{}.json", to_export[0].id))
            };

            let content = if export_all {
                match serde_json::to_string_pretty(&to_export) {
                    Ok(s) => s,
                    Err(e) => {
                        ui_logger.log(format!(
                            "{} Failed to serialize stories: {e}",
                            Icons::cross()
                        ));
                        return;
                    }
                }
            } else {
                match serde_json::to_string_pretty(to_export[0]) {
                    Ok(s) => s,
                    Err(e) => {
                        ui_logger.log(format!("{} Failed to serialize story: {e}", Icons::cross()));
                        return;
                    }
                }
            };

            if let Err(e) = tokio::fs::write(&path, content).await {
                error_logger.log_error(&format!("Failed to write export file: {e}"));
                ui_logger.log(format!("{} Failed to write file: {e}", Icons::cross()));
                return;
            }

            ui_logger.log(format!(
                "{} Exported {} {} to {}",
                Icons::book(),
                to_export.len(),
                if to_export.len() == 1 {
                    "story"
                } else {
                    "stories"
                },
                path.display()
            ));
        }
        "md" => {
            let mut exported = 0usize;
            for story in &to_export {
                let safe_name: String = story
                    .name
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() || c == '-' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                let filename = format!("story_{}_{}.md", story.id, safe_name);
                let path = export_dir.join(&filename);

                let content = format!(
                    "---\n\
                     id: {id}\n\
                     channel: {channel}\n\
                     public: {public}\n\
                     created_at: {created_at}\n\
                     ---\n\n\
                     # {name}\n\n\
                     ## Header\n\n\
                     {header}\n\n\
                     ## Body\n\n\
                     {body}\n",
                    id = story.id,
                    channel = story.channel,
                    public = story.public,
                    created_at = story.created_at,
                    name = story.name,
                    header = story.header,
                    body = story.body
                );

                if let Err(e) = tokio::fs::write(&path, content).await {
                    error_logger.log_error(&format!("Failed to write {filename}: {e}"));
                    ui_logger.log(format!(
                        "{} Failed to write {filename}: {e}",
                        Icons::cross()
                    ));
                    continue;
                }
                exported += 1;
            }

            ui_logger.log(format!(
                "{} Exported {}/{} {} as Markdown to {}/",
                Icons::book(),
                exported,
                to_export.len(),
                if to_export.len() == 1 {
                    "story"
                } else {
                    "stories"
                },
                export_dir.display()
            ));
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Story;

    fn make_story(id: usize, name: &str, public: bool, channel: &str) -> Story {
        Story {
            id,
            name: name.to_string(),
            header: "h".to_string(),
            body: "b".to_string(),
            public,
            channel: channel.to_string(),
            created_at: 0,
            auto_share: None,
        }
    }

    #[test]
    fn test_format_story_status_public() {
        let s = format_story_status(true);
        assert!(s.contains("Public"));
    }

    #[test]
    fn test_format_story_status_private() {
        let s = format_story_status(false);
        assert!(s.contains("Private"));
    }

    #[test]
    fn test_format_story_line_contains_fields() {
        let story = make_story(1, "My Story", true, "general");
        let line = format_story_line(&story);
        assert!(line.contains("general"));
        assert!(line.contains("1"));
        assert!(line.contains("My Story"));
        assert!(line.contains("Public"));
    }

    #[test]
    fn test_format_story_line_no_channel_omits_channel() {
        let story = make_story(2, "Test", false, "tech");
        let line = format_story_line_no_channel(&story);
        assert!(!line.contains("tech"));
        assert!(line.contains("Private"));
        assert!(line.contains("Test"));
    }
}
