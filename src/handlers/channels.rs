//! Channel command handlers: create, list, subscribe, unsubscribe, auto-subscription.

use crate::error_logger::ErrorLogger;
use crate::network::{PEER_ID, StoryBehaviour, TOPIC};
use crate::storage::{
    create_channel, read_channels, read_subscribed_channels, read_unsubscribed_channels,
    subscribe_to_channel, unsubscribe_from_channel,
};
use crate::types::{ActionResult, Icons};
use crate::validation::ContentValidator;
use bytes::Bytes;
use libp2p::swarm::Swarm;

use super::{UILogger, modify_config, validate_and_log};

// ---------------------------------------------------------------------------
// Shared formatting helpers
// ---------------------------------------------------------------------------

/// One-line channel summary: `  <name> - <description>`.
fn format_channel_line(name: &str, description: &str) -> String {
    format!("  {} - {}", name, description)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn handle_create_channel(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    local_peer_name: &Option<String>,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<ActionResult> {
    if let Some(rest) = cmd.strip_prefix("create ch") {
        let rest = rest.trim();
        let elements: Vec<&str> = rest.split('|').collect();

        if elements.len() < 2 {
            ui_logger.log("Format: create ch name|description".to_string());
            return None;
        }

        let name = elements[0].trim();
        let description = elements[1].trim();

        let validated_name = validate_and_log(
            ContentValidator::validate_channel_name(name),
            "channel name",
            ui_logger,
        )?;
        let validated_description = validate_and_log(
            ContentValidator::validate_channel_description(description),
            "channel description",
            ui_logger,
        )?;

        let creator = match local_peer_name {
            Some(peer_name) => peer_name.clone(),
            None => PEER_ID.to_string(),
        };

        if let Err(e) = create_channel(&validated_name, &validated_description, &creator).await {
            error_logger.log_error(&format!("Failed to create channel: {e}"));
        } else {
            ui_logger.log(format!("Channel '{validated_name}' created successfully"));

            if let Err(e) = subscribe_to_channel(&PEER_ID.to_string(), &validated_name).await {
                error_logger
                    .log_error(&format!("Failed to auto-subscribe to created channel: {e}"));
            }

            let channel = crate::types::Channel::new(
                validated_name.clone(),
                validated_description,
                creator.clone(),
            );
            let published_channel = crate::types::PublishedChannel::new(channel.clone(), creator);

            let published_json = match serde_json::to_string(&published_channel) {
                Ok(json) => json,
                Err(e) => {
                    error_logger.log_error(&format!("Failed to serialize published channel: {e}"));
                    return Some(ActionResult::RefreshStories);
                }
            };
            let published_json_bytes = Bytes::from(published_json.into_bytes());
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), published_json_bytes);

            let legacy_json = match serde_json::to_string(&channel) {
                Ok(json) => json,
                Err(e) => {
                    error_logger.log_error(&format!("Failed to serialize legacy channel: {e}"));
                    return Some(ActionResult::RefreshStories);
                }
            };
            let legacy_json_bytes = Bytes::from(legacy_json.into_bytes());
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), legacy_json_bytes);

            ui_logger.log(format!("Channel '{validated_name}' shared with network"));

            return Some(ActionResult::RefreshStories);
        }
    }
    None
}

pub async fn handle_list_channels(cmd: &str, ui_logger: &UILogger, error_logger: &ErrorLogger) {
    let rest = cmd.strip_prefix("ls ch");
    match rest {
        Some(" available") => match read_channels().await {
            Ok(channels) => {
                ui_logger.log("Available channels:".to_string());
                if channels.is_empty() {
                    ui_logger.log("  (no channels discovered)".to_string());
                } else {
                    for channel in channels {
                        ui_logger.log(format_channel_line(&channel.name, &channel.description));
                    }
                }
            }
            Err(e) => error_logger.log_error(&format!("Failed to read available channels: {e}")),
        },
        Some(" unsubscribed") => match read_unsubscribed_channels(&PEER_ID.to_string()).await {
            Ok(channels) => {
                ui_logger.log("Unsubscribed channels:".to_string());
                if channels.is_empty() {
                    ui_logger.log("  (no unsubscribed channels)".to_string());
                } else {
                    for channel in channels {
                        ui_logger.log(format_channel_line(&channel.name, &channel.description));
                    }
                }
            }
            Err(e) => {
                error_logger.log_error(&format!("Failed to read unsubscribed channels: {e}"))
            }
        },
        Some("") | None => match read_channels().await {
            Ok(channels) => {
                ui_logger.log("Available channels:".to_string());
                if channels.is_empty() {
                    ui_logger.log("  (no channels discovered)".to_string());
                } else {
                    for channel in channels {
                        ui_logger.log(format_channel_line(&channel.name, &channel.description));
                    }
                }
            }
            Err(e) => error_logger.log_error(&format!("Failed to read channels: {e}")),
        },
        _ => {
            ui_logger.log("Usage: ls ch [available|unsubscribed]".to_string());
        }
    }
}

pub async fn handle_subscribe_channel(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<crate::types::ActionResult> {
    let channel_name = if let Some(name) = cmd.strip_prefix("sub ch ") {
        name.trim()
    } else if let Some(name) = cmd.strip_prefix("sub ") {
        name.trim()
    } else {
        ui_logger.log("Usage: sub ch <channel_name> or sub <channel_name>".to_string());
        return None;
    };

    if channel_name.is_empty() {
        ui_logger.log("Usage: sub ch <channel_name> or sub <channel_name>".to_string());
        return None;
    }

    match read_channels().await {
        Ok(channels) => {
            let channel_exists = channels.iter().any(|c| c.name == channel_name);
            if !channel_exists {
                ui_logger.log(format!("{} Channel '{channel_name}' not found in available channels. Use 'ls ch available' to see all discovered channels.", Icons::cross()));
                return None;
            }
        }
        Err(e) => {
            error_logger.log_error(&format!("Failed to check available channels: {e}"));
            ui_logger.log(format!(
                "{} Could not verify channel exists. Please try again.",
                Icons::cross()
            ));
            return None;
        }
    }

    if let Err(e) = subscribe_to_channel(&PEER_ID.to_string(), channel_name).await {
        error_logger.log_error(&format!("Failed to subscribe to channel: {e}"));
        ui_logger.log(format!(
            "Failed to subscribe to channel '{channel_name}': {e}"
        ));
        None
    } else {
        ui_logger.log(format!(
            "{} Subscribed to channel '{channel_name}'",
            Icons::check()
        ));
        Some(crate::types::ActionResult::RefreshChannels)
    }
}

pub async fn handle_unsubscribe_channel(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) -> Option<crate::types::ActionResult> {
    let channel_name = if let Some(name) = cmd.strip_prefix("unsub ch ") {
        name.trim()
    } else if let Some(name) = cmd.strip_prefix("unsub ") {
        name.trim()
    } else {
        ui_logger.log("Usage: unsub ch <channel_name> or unsub <channel_name>".to_string());
        return None;
    };

    if channel_name.is_empty() {
        ui_logger.log("Usage: unsub ch <channel_name> or unsub <channel_name>".to_string());
        return None;
    }

    if let Err(e) = unsubscribe_from_channel(&PEER_ID.to_string(), channel_name).await {
        error_logger.log_error(&format!("Failed to unsubscribe from channel: {e}"));
        ui_logger.log(format!(
            "Failed to unsubscribe from channel '{channel_name}': {e}"
        ));
        None
    } else {
        ui_logger.log(format!(
            "{} Unsubscribed from channel '{channel_name}'",
            Icons::check()
        ));
        Some(crate::types::ActionResult::RefreshChannels)
    }
}

pub async fn handle_list_subscriptions(ui_logger: &UILogger, error_logger: &ErrorLogger) {
    match read_subscribed_channels(&PEER_ID.to_string()).await {
        Ok(channels) => {
            ui_logger.log("Your subscribed channels:".to_string());
            if channels.is_empty() {
                ui_logger.log("  (no subscriptions)".to_string());
            } else {
                for channel in channels {
                    ui_logger.log(format!("  {channel}"));
                }
            }
        }
        Err(e) => error_logger.log_error(&format!("Failed to read subscriptions: {e}")),
    }
}

pub async fn handle_set_auto_subscription(
    cmd: &str,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) {
    let rest = cmd.strip_prefix("set auto-sub ");
    match rest {
        Some("on") => {
            if modify_config(ui_logger, error_logger, "auto-subscription", |config| {
                config
                    .channel_auto_subscription
                    .auto_subscribe_to_new_channels = true;
            })
            .await
            {
                ui_logger.log(format!("{} Auto-subscription enabled", Icons::check()));
            }
        }
        Some("off") => {
            if modify_config(ui_logger, error_logger, "auto-subscription", |config| {
                config
                    .channel_auto_subscription
                    .auto_subscribe_to_new_channels = false;
            })
            .await
            {
                ui_logger.log(format!("{} Auto-subscription disabled", Icons::cross()));
            }
        }
        Some("status") | None => match crate::storage::load_unified_network_config().await {
            Ok(config) => {
                let status = if config
                    .channel_auto_subscription
                    .auto_subscribe_to_new_channels
                {
                    "enabled"
                } else {
                    "disabled"
                };
                ui_logger.log(format!("Auto-subscription is currently {status}"));
                ui_logger.log(format!(
                    "Notifications: {}",
                    if config.channel_auto_subscription.notify_new_channels {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ));
                ui_logger.log(format!(
                    "Max auto-subscriptions: {}",
                    config.channel_auto_subscription.max_auto_subscriptions
                ));
            }
            Err(e) => {
                error_logger.log_error(&format!("Failed to load config: {e}"));
                ui_logger.log(format!("{} Failed to load configuration", Icons::cross()));
            }
        },
        _ => {
            ui_logger.log("Usage: set auto-sub [on|off|status]".to_string());
        }
    }
}
