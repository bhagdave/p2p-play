//! Direct messaging handlers: `msg`, `compose`, peer name, relay delivery, retry queue.

use crate::errors::CryptoError;
use crate::network::{DirectMessageRequest, PEER_ID, StoryBehaviour};
use crate::relay::{RelayError, RelayService};
use crate::storage::save_local_peer_name;
use crate::types::{DirectMessage, DirectMessageConfig, Icons, PeerName, PendingDirectMessage};
use crate::validation::ContentValidator;
use libp2p::PeerId;
use libp2p::swarm::Swarm;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::{UILogger, current_unix_timestamp, resolve_peer_by_alias, validate_and_log};

pub async fn handle_set_name(
    cmd: &str,
    local_peer_name: &mut Option<String>,
    ui_logger: &UILogger,
) -> Option<PeerName> {
    if let Some(name) = cmd.strip_prefix("name ") {
        let name = name.trim();

        let validated_name = validate_and_log(
            ContentValidator::validate_peer_name(name),
            "peer name",
            ui_logger,
        )?;

        *local_peer_name = Some(validated_name.clone());

        if let Err(e) = save_local_peer_name(&validated_name).await {
            ui_logger.log(format!("Warning: Failed to save peer name: {e}"));
        }

        Some(PeerName::new(PEER_ID.to_string(), validated_name))
    } else {
        ui_logger.log("Usage: name <alias>".to_string());
        None
    }
}

/// Parses the rest of a `msg <peer_alias> <message>` command.
///
/// `sorted_peer_names` must be sorted longest-first so that longer names take precedence
/// over shorter prefix matches.  Returns `(peer_alias, message)` on success.
pub fn parse_direct_message_command(
    rest: &str,
    sorted_peer_names: &[String],
) -> Option<(String, String)> {
    for peer_name in sorted_peer_names {
        if rest.starts_with(peer_name) {
            let remaining = &rest[peer_name.len()..];

            if remaining.is_empty() {
                return None;
            }

            if let Some(stripped) = remaining.strip_prefix(' ') {
                let message = stripped.trim();
                if !message.is_empty() {
                    return Some((peer_name.clone(), message.to_string()));
                } else {
                    return None;
                }
            }

            // Peer name not followed by a space → invalid
            return None;
        }
    }

    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
    if parts.len() >= 2 {
        let to_name = parts[0].trim();
        let message = parts[1].trim();

        if !to_name.is_empty() && !message.is_empty() {
            return Some((to_name.to_string(), message.to_string()));
        }
    }

    None
}

pub async fn handle_direct_message_with_relay(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
    local_peer_name: &Option<String>,
    sorted_peer_names_cache: &super::SortedPeerNamesCache,
    ui_logger: &UILogger,
    dm_config: &DirectMessageConfig,
    relay_service: &mut Option<RelayService>,
    pending_messages: &Arc<Mutex<Vec<PendingDirectMessage>>>,
) {
    if let Some(rest) = cmd.strip_prefix("msg ") {
        let (to_name, message) =
            match parse_direct_message_command(rest, sorted_peer_names_cache.get_sorted_names()) {
                Some((name, msg)) => (name, msg),
                None => {
                    ui_logger.log("Usage: msg <peer_alias> <message>".to_string());
                    return;
                }
            };

        if to_name.is_empty() || message.is_empty() {
            ui_logger.log("Both peer alias and message must be non-empty".to_string());
            return;
        }

        let from_name = match local_peer_name {
            Some(name) => name.clone(),
            None => {
                ui_logger.log("You must set your name first using 'name <alias>'".to_string());
                return;
            }
        };

        let _validated_to_name = match validate_and_log(
            ContentValidator::validate_peer_name(&to_name),
            "peer name",
            ui_logger,
        ) {
            Some(name) => name,
            None => return,
        };

        let message = match validate_and_log(
            ContentValidator::validate_direct_message(&message),
            "direct message",
            ui_logger,
        ) {
            Some(msg) => msg,
            None => return,
        };

        let target_peer_info = resolve_peer_by_alias(&to_name, peer_names)
            .map(|peer_id| (peer_id, false));

        if let Some((target_peer_id, _)) = target_peer_info {
            let prefer_direct = relay_service
                .as_ref()
                .map(|rs| rs.config().prefer_direct)
                .unwrap_or(true);

            if prefer_direct {
                ui_logger.log(format!(
                    "{} Attempting direct message to {to_name}...",
                    Icons::speech()
                ));

                let direct_msg_request = DirectMessageRequest {
                    from_peer_id: PEER_ID.to_string(),
                    from_name: from_name.clone(),
                    to_name: to_name.clone(),
                    message: message.clone(),
                    timestamp: current_unix_timestamp(),
                };

                let request_id = swarm
                    .behaviour_mut()
                    .request_response
                    .send_request(&target_peer_id, direct_msg_request.clone());

                let outgoing_message = crate::types::DirectMessage {
                    from_peer_id: direct_msg_request.from_peer_id.clone(),
                    from_name: direct_msg_request.from_name.clone(),
                    to_peer_id: target_peer_id.to_string(),
                    to_name: direct_msg_request.to_name.clone(),
                    message: direct_msg_request.message.clone(),
                    timestamp: direct_msg_request.timestamp,
                    is_outgoing: true,
                };

                if let Err(e) =
                    crate::storage::save_direct_message(&outgoing_message, Some(peer_names)).await
                {
                    ui_logger.log(format!("Failed to save outgoing message: {}", e));
                } else {
                    ui_logger.log(format!("Saved outgoing message to {}", to_name));
                }

                ui_logger.log(format!(
                    "Direct message sent to {to_name} (request_id: {request_id:?})"
                ));

                return;
            }
        }

        // Try relay delivery if relay service is available and enabled
        if let Some(relay_svc) = relay_service
            && relay_svc.config().enable_relay
        {
            let relay_target_peer_id = if let Some((peer_id, _)) = target_peer_info {
                peer_id
            } else {
                ui_logger.log(format!(
                    "{} Cannot relay to unknown peer '{to_name}' - peer not in network",
                    Icons::cross()
                ));
                ui_logger.log(format!(
                    "{} Queueing message for {to_name} - will retry when peer connects",
                    Icons::envelope()
                ));
                queue_message_for_retry(
                    &from_name,
                    &to_name,
                    &message,
                    &target_peer_info,
                    dm_config,
                    pending_messages,
                    ui_logger,
                );
                return;
            };

            if try_relay_delivery(
                swarm,
                relay_svc,
                &from_name,
                &to_name,
                &message,
                &relay_target_peer_id,
                ui_logger,
            )
            .await
            {
                return;
            }
        }

        // Fall back to traditional queuing system for retry
        ui_logger.log(format!(
            "{} Queueing message for {to_name} - will retry when peer connects",
            Icons::envelope()
        ));
        queue_message_for_retry(
            &from_name,
            &to_name,
            &message,
            &target_peer_info,
            dm_config,
            pending_messages,
            ui_logger,
        );
    } else {
        ui_logger.log("Usage: msg <peer_alias> <message>".to_string());
    }
}

async fn try_relay_delivery(
    swarm: &mut Swarm<StoryBehaviour>,
    relay_service: &mut RelayService,
    from_name: &str,
    to_name: &str,
    message: &str,
    target_peer_id: &PeerId,
    ui_logger: &UILogger,
) -> bool {
    ui_logger.log(format!(
        "{} Trying relay delivery to {to_name}...",
        Icons::speech()
    ));

    let direct_msg = DirectMessage {
        from_peer_id: PEER_ID.to_string(),
        from_name: from_name.to_string(),
        to_peer_id: target_peer_id.to_string(),
        to_name: to_name.to_string(),
        message: message.to_string(),
        timestamp: current_unix_timestamp(),
        is_outgoing: true,
    };

    match relay_service.create_relay_message(&direct_msg, target_peer_id) {
        Ok(relay_msg) => {
            match crate::event_handlers::broadcast_relay_message(swarm, &relay_msg).await {
                Ok(()) => {
                    ui_logger.log(format!(
                        "{} Message sent to {to_name} via relay network",
                        Icons::check()
                    ));
                    true
                }
                Err(e) => {
                    ui_logger.log(format!(
                        "{} Failed to broadcast relay message: {e}",
                        Icons::cross()
                    ));
                    false
                }
            }
        }
        Err(e) => {
            if let RelayError::CryptoError(CryptoError::EncryptionFailed(msg)) = &e
                && msg.contains("Public key not found")
            {
                ui_logger.log(format!(
                    "{} Cannot send secure message to offline peer '{to_name}'",
                    Icons::warning()
                ));
                ui_logger.log(format!(
                        "{} Message queued - will be delivered when {to_name} comes online and security keys are exchanged",
                        Icons::envelope()
                    ));
                ui_logger.log(format!(
                    "{}  Tip: Both peers must be online simultaneously for secure messaging setup",
                    Icons::memo()
                ));
                return false;
            }

            ui_logger.log(format!(
                "{} Failed to create relay message: {e}",
                Icons::cross()
            ));
            false
        }
    }
}

fn queue_message_for_retry(
    from_name: &str,
    to_name: &str,
    message: &str,
    target_peer_info: &Option<(PeerId, bool)>,
    dm_config: &DirectMessageConfig,
    pending_messages: &Arc<Mutex<Vec<PendingDirectMessage>>>,
    ui_logger: &UILogger,
) {
    let (target_peer_id, is_placeholder) = target_peer_info.unwrap_or_else(|| {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        to_name.hash(&mut hasher);
        let placeholder_id =
            PeerId::from_bytes(&hasher.finish().to_be_bytes()).unwrap_or(PeerId::random());
        (placeholder_id, true)
    });

    let direct_msg_request = DirectMessageRequest {
        from_peer_id: PEER_ID.to_string(),
        from_name: from_name.to_string(),
        to_name: to_name.to_string(),
        message: message.to_string(),
        timestamp: current_unix_timestamp(),
    };

    let pending_msg = PendingDirectMessage::new(
        target_peer_id,
        to_name.to_string(),
        direct_msg_request,
        dm_config.max_retry_attempts,
        is_placeholder,
    );

    if let Ok(mut queue) = pending_messages.lock() {
        queue.push(pending_msg);
        ui_logger.log(format!("Message for {to_name} added to retry queue"));
    } else {
        ui_logger.log("Failed to queue message - retry system unavailable".to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dm_basic() {
        let names = vec!["alice".to_string(), "bob".to_string()];
        let result = parse_direct_message_command("alice Hello there", &names);
        assert_eq!(result, Some(("alice".to_string(), "Hello there".to_string())));
    }

    #[test]
    fn test_parse_dm_unknown_peer_falls_back_to_split() {
        let names: Vec<String> = vec![];
        let result = parse_direct_message_command("charlie hi", &names);
        assert_eq!(result, Some(("charlie".to_string(), "hi".to_string())));
    }

    #[test]
    fn test_parse_dm_no_message_returns_none() {
        let names = vec!["alice".to_string()];
        let result = parse_direct_message_command("alice", &names);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_dm_empty_message_returns_none() {
        let names = vec!["alice".to_string()];
        let result = parse_direct_message_command("alice   ", &names);
        // The fallback splitn path: "alice   " → parts[0]="alice", parts[1]="  " which trims to ""
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_dm_longer_name_wins_over_prefix() {
        // "alice bob" should match "alice bob" (longer) over "alice" (shorter prefix)
        let mut names = vec!["alice".to_string(), "alice bob".to_string()];
        // sort longest-first as the real cache does
        names.sort_by_key(|b| std::cmp::Reverse(b.len()));
        let result = parse_direct_message_command("alice bob Hello", &names);
        assert_eq!(
            result,
            Some(("alice bob".to_string(), "Hello".to_string()))
        );
    }

    #[test]
    fn test_parse_dm_name_not_followed_by_space_returns_none() {
        let names = vec!["alice".to_string()];
        // "alicebob" starts with "alice" but next char is 'b', not ' '
        let result = parse_direct_message_command("alicebob hey", &names);
        // "alicebob" starts_with "alice" → remaining="bob hey" → no leading space → None
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_dm_empty_input_returns_none() {
        let names: Vec<String> = vec![];
        let result = parse_direct_message_command("", &names);
        assert!(result.is_none());
    }
}
