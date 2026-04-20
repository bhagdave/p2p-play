//! WASM capability command handlers.

use crate::error_logger::ErrorLogger;
use crate::network::{PEER_ID, StoryBehaviour, WasmCapabilitiesRequest, WasmExecutionRequest};
use crate::types::Icons;
use crate::validation::ContentValidator;
use libp2p::PeerId;
use libp2p::swarm::Swarm;
use std::collections::HashMap;

use super::{UILogger, current_unix_timestamp, load_config_or_log, modify_config, resolve_connected_peer, validate_and_log};

/// Dispatch `wasm <subcommand>` commands.
pub async fn handle_wasm_command(
    cmd: &str,
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
    local_peer_name: &Option<String>,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) {
    let parts: Vec<&str> = cmd.split_whitespace().collect();

    if parts.len() < 2 {
        show_wasm_usage(ui_logger);
        return;
    }

    match parts[1] {
        "create" => handle_wasm_create(&parts[2..], ui_logger, error_logger).await,
        "ls" | "list" => handle_wasm_list(&parts[2..], ui_logger, error_logger).await,
        "show" => handle_wasm_show(&parts[2..], ui_logger, error_logger).await,
        "toggle" => handle_wasm_toggle(&parts[2..], ui_logger, error_logger).await,
        "delete" => handle_wasm_delete(&parts[2..], ui_logger, error_logger).await,
        "param" => handle_wasm_param(&parts[2..], ui_logger, error_logger).await,
        "query" => {
            handle_wasm_query(&parts[2..], swarm, peer_names, local_peer_name, ui_logger).await
        }
        "run" => {
            handle_wasm_run(
                &parts[2..],
                swarm,
                peer_names,
                local_peer_name,
                ui_logger,
                error_logger,
            )
            .await
        }
        "config" => handle_wasm_config(&parts[2..], ui_logger, error_logger).await,
        _ => show_wasm_usage(ui_logger),
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn show_wasm_usage(ui_logger: &UILogger) {
    ui_logger.log("WASM Commands:".to_string());
    ui_logger.log(
        "  wasm create <name>|<description>|<ipfs_cid>|<version> - Create new offering".to_string(),
    );
    ui_logger.log(
        "  wasm ls [local|remote|all]                            - List offerings".to_string(),
    );
    ui_logger.log(
        "  wasm show <id>                                        - Show offering details"
            .to_string(),
    );
    ui_logger.log(
        "  wasm toggle <id>                                      - Enable/disable offering"
            .to_string(),
    );
    ui_logger.log(
        "  wasm delete <id>                                      - Delete local offering"
            .to_string(),
    );
    ui_logger.log(
        "  wasm param add <id> <name>|<type>|<desc>|<required>   - Add parameter to offering"
            .to_string(),
    );
    ui_logger.log(
        "  wasm query <peer_alias>                               - Query peer's WASM capabilities"
            .to_string(),
    );
    ui_logger.log(
        "  wasm run <peer_alias> <offering_id> [args...]         - Execute WASM on remote peer"
            .to_string(),
    );
    ui_logger.log(
        "  wasm config [advertise|execute] [on|off|status]       - Configure WASM settings"
            .to_string(),
    );
}

async fn handle_wasm_create(args: &[&str], ui_logger: &UILogger, error_logger: &ErrorLogger) {
    if args.is_empty() {
        ui_logger.usage("wasm create <name>|<description>|<ipfs_cid>|<version>");
        return;
    }

    let full_arg = args.join(" ");
    let parts: Vec<&str> = full_arg.split('|').collect();

    if parts.len() < 4 {
        ui_logger.usage("wasm create <name>|<description>|<ipfs_cid>|<version>");
        ui_logger.log(
            "Example: wasm create echo-module|A simple echo module|QmXyz123|1.0.0".to_string(),
        );
        return;
    }

    let name = parts[0].trim();
    let description = parts[1].trim();
    let ipfs_cid = parts[2].trim();
    let version = parts[3].trim();

    let validated_name = match validate_and_log(
        ContentValidator::validate_wasm_offering_name(name),
        "offering name",
        ui_logger,
    ) {
        Some(n) => n,
        None => return,
    };

    let validated_description = match validate_and_log(
        ContentValidator::validate_wasm_offering_description(description),
        "offering description",
        ui_logger,
    ) {
        Some(d) => d,
        None => return,
    };

    let validated_cid = match validate_and_log(
        ContentValidator::validate_ipfs_cid(ipfs_cid),
        "IPFS CID",
        ui_logger,
    ) {
        Some(c) => c,
        None => return,
    };

    let validated_version = match validate_and_log(
        ContentValidator::validate_wasm_version(version),
        "version",
        ui_logger,
    ) {
        Some(v) => v,
        None => return,
    };

    let offering = crate::types::WasmOffering::new(
        validated_name.clone(),
        validated_description,
        validated_cid,
        validated_version,
    );

    match crate::storage::create_wasm_offering(&offering).await {
        Ok(_) => {
            ui_logger.log(format!(
                "{} WASM offering '{}' created successfully (ID: {})",
                Icons::check(),
                validated_name,
                offering.id
            ));
        }
        Err(e) => {
            error_logger.log_error(&format!("Failed to create WASM offering: {e}"));
            ui_logger.log(format!("{} Failed to create offering: {e}", Icons::cross()));
        }
    }
}

/// Print one offering row.
fn print_offering(offering: &crate::types::WasmOffering, ui_logger: &UILogger) {
    let status = if offering.enabled {
        format!("{} enabled", Icons::check())
    } else {
        format!("{} disabled", Icons::cross())
    };
    ui_logger.log(format!(
        "  {} {} (v{}) [{}] - {}",
        Icons::chart(),
        offering.name,
        offering.version,
        status,
        offering.id
    ));
}

async fn handle_wasm_list(args: &[&str], ui_logger: &UILogger, error_logger: &ErrorLogger) {
    let filter = args.first().copied().unwrap_or("all");

    match filter {
        "local" => print_local_offerings(ui_logger, error_logger).await,
        "remote" => print_remote_offerings(ui_logger, error_logger).await,
        // "all" or any unrecognised value shows both sections
        "all" | _ => {
            ui_logger.log("=== Local Offerings ===".to_string());
            print_local_offerings(ui_logger, error_logger).await;
            ui_logger.log("".to_string());
            ui_logger.log("=== Remote Offerings ===".to_string());
            print_remote_offerings(ui_logger, error_logger).await;
        }
    }
}

async fn print_local_offerings(ui_logger: &UILogger, error_logger: &ErrorLogger) {
    match crate::storage::read_wasm_offerings().await {
        Ok(offerings) => {
            ui_logger.log(format!("Local WASM Offerings ({}):", offerings.len()));
            if offerings.is_empty() {
                ui_logger.log("  (no local offerings)".to_string());
            } else {
                for offering in &offerings {
                    print_offering(offering, ui_logger);
                }
            }
        }
        Err(e) => {
            error_logger.log_error(&format!("Failed to list local WASM offerings: {e}"));
            ui_logger.log(format!("{} Failed to list offerings", Icons::cross()));
        }
    }
}

async fn print_remote_offerings(ui_logger: &UILogger, error_logger: &ErrorLogger) {
    match crate::storage::get_all_cached_wasm_offerings().await {
        Ok(offerings) => {
            ui_logger.log(format!("Discovered WASM Offerings ({}):", offerings.len()));
            if offerings.is_empty() {
                ui_logger.log(
                    "  (no discovered offerings - use 'wasm query <peer>' to discover)".to_string(),
                );
            } else {
                let mut current_peer = String::new();
                for (peer_id, offering) in offerings {
                    if peer_id != current_peer {
                        ui_logger.log(format!(
                            "  From peer {}:",
                            &peer_id[..12.min(peer_id.len())]
                        ));
                        current_peer = peer_id;
                    }
                    ui_logger.log(format!(
                        "    {} {} (v{}) - {}",
                        Icons::chart(),
                        offering.name,
                        offering.version,
                        offering.description
                    ));
                }
            }
        }
        Err(e) => {
            error_logger.log_error(&format!("Failed to list cached WASM offerings: {e}"));
            ui_logger.log(format!("{} Failed to list offerings", Icons::cross()));
        }
    }
}

async fn handle_wasm_show(args: &[&str], ui_logger: &UILogger, error_logger: &ErrorLogger) {
    if args.is_empty() {
        ui_logger.usage("wasm show <id>");
        return;
    }

    let id = args[0];

    match crate::storage::get_wasm_offering_by_id(id).await {
        Ok(Some(offering)) => {
            ui_logger.log(format!("{} WASM Offering Details:", Icons::chart()));
            ui_logger.log(format!("  ID: {}", offering.id));
            ui_logger.log(format!("  Name: {}", offering.name));
            ui_logger.log(format!("  Description: {}", offering.description));
            ui_logger.log(format!("  Version: {}", offering.version));
            ui_logger.log(format!("  IPFS CID: {}", offering.ipfs_cid));
            ui_logger.log(format!(
                "  Status: {}",
                if offering.enabled { "enabled" } else { "disabled" }
            ));
            ui_logger.log(format!("  Parameters: {}", offering.parameters.len()));
            for param in &offering.parameters {
                ui_logger.log(format!(
                    "    - {} ({}): {} {}",
                    param.name,
                    param.param_type,
                    param.description,
                    if param.required { "[required]" } else { "[optional]" }
                ));
            }
            ui_logger.log(format!(
                "  Resource Requirements: fuel {}-{}, memory {}-{}MB, timeout {}s",
                offering.resource_requirements.min_fuel,
                offering.resource_requirements.max_fuel,
                offering.resource_requirements.min_memory_mb,
                offering.resource_requirements.max_memory_mb,
                offering.resource_requirements.estimated_timeout_secs
            ));
        }
        Ok(None) => {
            ui_logger.log(format!("Offering with ID '{}' not found", id));
        }
        Err(e) => {
            error_logger.log_error(&format!("Failed to get WASM offering: {e}"));
            ui_logger.log(format!("{} Failed to get offering", Icons::cross()));
        }
    }
}

async fn handle_wasm_toggle(args: &[&str], ui_logger: &UILogger, error_logger: &ErrorLogger) {
    if args.is_empty() {
        ui_logger.usage("wasm toggle <id>");
        return;
    }

    let id = args[0];

    match crate::storage::get_wasm_offering_by_id(id).await {
        Ok(Some(offering)) => {
            let new_state = !offering.enabled;
            match crate::storage::toggle_wasm_offering(id, new_state).await {
                Ok(true) => {
                    ui_logger.log(format!(
                        "{} Offering '{}' is now {}",
                        if new_state { Icons::check() } else { Icons::cross() },
                        offering.name,
                        if new_state { "enabled" } else { "disabled" }
                    ));
                }
                Ok(false) => {
                    ui_logger.log("Offering not found".to_string());
                }
                Err(e) => {
                    error_logger.log_error(&format!("Failed to toggle offering: {e}"));
                    ui_logger.log(format!("{} Failed to toggle offering", Icons::cross()));
                }
            }
        }
        Ok(None) => {
            ui_logger.log(format!("Offering with ID '{}' not found", id));
        }
        Err(e) => {
            error_logger.log_error(&format!("Failed to get offering: {e}"));
            ui_logger.log(format!("{} Failed to get offering", Icons::cross()));
        }
    }
}

async fn handle_wasm_delete(args: &[&str], ui_logger: &UILogger, error_logger: &ErrorLogger) {
    if args.is_empty() {
        ui_logger.usage("wasm delete <id>");
        return;
    }

    let id = args[0];

    match crate::storage::delete_wasm_offering(id).await {
        Ok(true) => {
            ui_logger.log(format!("{} Offering deleted successfully", Icons::check()));
        }
        Ok(false) => {
            ui_logger.log(format!("Offering with ID '{}' not found", id));
        }
        Err(e) => {
            error_logger.log_error(&format!("Failed to delete offering: {e}"));
            ui_logger.log(format!("{} Failed to delete offering", Icons::cross()));
        }
    }
}

async fn handle_wasm_param(args: &[&str], ui_logger: &UILogger, error_logger: &ErrorLogger) {
    if args.len() < 2 {
        ui_logger.usage(
            "wasm param add <offering_id> <name>|<type>|<description>|<required>",
        );
        return;
    }

    match args[0] {
        "add" => {
            if args.len() < 3 {
                ui_logger.usage(
                    "wasm param add <offering_id> <name>|<type>|<description>|<required>",
                );
                return;
            }

            let offering_id = args[1];
            let param_spec = args[2..].join(" ");
            let parts: Vec<&str> = param_spec.split('|').collect();

            if parts.len() < 4 {
                ui_logger.usage("wasm param add <id> <name>|<type>|<description>|<required>");
                ui_logger.log("Types: string, bytes, json, int, float, bool, file".to_string());
                return;
            }

            let param_name = parts[0].trim();
            let param_type = parts[1].trim();
            let param_desc = parts[2].trim();
            let required_str = parts[3].trim().to_lowercase();

            let validated_name = match validate_and_log(
                ContentValidator::validate_wasm_param_name(param_name),
                "parameter name",
                ui_logger,
            ) {
                Some(n) => n,
                None => return,
            };

            let validated_type = match validate_and_log(
                ContentValidator::validate_wasm_param_type(param_type),
                "parameter type",
                ui_logger,
            ) {
                Some(t) => t,
                None => return,
            };

            let required = matches!(required_str.as_str(), "true" | "yes" | "1" | "required");

            match crate::storage::get_wasm_offering_by_id(offering_id).await {
                Ok(Some(mut offering)) => {
                    let new_param = crate::types::WasmParameter::new(
                        validated_name.clone(),
                        validated_type,
                        param_desc.to_string(),
                        required,
                    );
                    offering.parameters.push(new_param);

                    match crate::storage::update_wasm_offering(&offering).await {
                        Ok(true) => {
                            ui_logger.log(format!(
                                "{} Added parameter '{}' to offering '{}'",
                                Icons::check(),
                                validated_name,
                                offering.name
                            ));
                        }
                        Ok(false) => {
                            ui_logger.log("Failed to update offering".to_string());
                        }
                        Err(e) => {
                            error_logger.log_error(&format!("Failed to update offering: {e}"));
                            ui_logger.log(format!("{} Failed to add parameter", Icons::cross()));
                        }
                    }
                }
                Ok(None) => {
                    ui_logger.log(format!("Offering with ID '{}' not found", offering_id));
                }
                Err(e) => {
                    error_logger.log_error(&format!("Failed to get offering: {e}"));
                    ui_logger.log(format!("{} Failed to get offering", Icons::cross()));
                }
            }
        }
        _ => {
            ui_logger.usage(
                "wasm param add <offering_id> <name>|<type>|<description>|<required>",
            );
        }
    }
}

async fn handle_wasm_query(
    args: &[&str],
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
    local_peer_name: &Option<String>,
    ui_logger: &UILogger,
) {
    if args.is_empty() {
        ui_logger.usage("wasm query <peer_alias>");
        return;
    }

    let peer_alias = args[0];

    let target_peer = match resolve_connected_peer(peer_alias, peer_names, swarm, ui_logger) {
        Some(peer) => peer,
        None => return,
    };

    let from_name = local_peer_name
        .clone()
        .unwrap_or_else(|| PEER_ID.to_string());

    let request = WasmCapabilitiesRequest {
        from_peer_id: PEER_ID.to_string(),
        from_name,
        timestamp: current_unix_timestamp(),
        include_parameters: true,
    };

    let _request_id = swarm
        .behaviour_mut()
        .wasm_capabilities
        .send_request(&target_peer, request);

    ui_logger.log(format!(
        "{} Querying WASM capabilities from '{}'...",
        Icons::sync(),
        peer_alias
    ));
}

async fn handle_wasm_run(
    args: &[&str],
    swarm: &mut Swarm<StoryBehaviour>,
    peer_names: &HashMap<PeerId, String>,
    local_peer_name: &Option<String>,
    ui_logger: &UILogger,
    error_logger: &ErrorLogger,
) {
    if args.len() < 2 {
        ui_logger.usage("wasm run <peer_alias> <offering_id> [args...]");
        return;
    }

    let peer_alias = args[0];
    let offering_id = args[1];
    let run_args: Vec<String> = args[2..].iter().map(|s| s.to_string()).collect();

    let target_peer = match resolve_connected_peer(peer_alias, peer_names, swarm, ui_logger) {
        Some(peer) => peer,
        None => return,
    };

    let cached_offerings =
        match crate::storage::get_cached_wasm_offerings_by_peer(&target_peer.to_string()).await {
            Ok(offerings) => offerings,
            Err(e) => {
                error_logger.log_error(&format!("Failed to get cached offerings: {e}"));
                ui_logger.log(format!(
                    "{} Failed to find offering. Try 'wasm query {}' first.",
                    Icons::cross(),
                    peer_alias
                ));
                return;
            }
        };

    let ipfs_cid = match cached_offerings.iter().find(|o| o.id == offering_id) {
        Some(o) => o.ipfs_cid.clone(),
        None => {
            ui_logger.log(format!(
                "{} Offering '{}' not found in cache for peer '{}'. Use 'wasm query {}' to refresh.",
                Icons::cross(),
                offering_id,
                peer_alias,
                peer_alias
            ));
            return;
        }
    };

    let from_name = local_peer_name
        .clone()
        .unwrap_or_else(|| PEER_ID.to_string());

    let request = WasmExecutionRequest {
        from_peer_id: PEER_ID.to_string(),
        from_name,
        offering_id: offering_id.to_string(),
        ipfs_cid,
        input: Vec::new(),
        args: run_args,
        fuel_limit: None,
        memory_limit_mb: None,
        timeout_secs: None,
        timestamp: current_unix_timestamp(),
    };

    let _request_id = swarm
        .behaviour_mut()
        .wasm_execution
        .send_request(&target_peer, request);

    ui_logger.log(format!(
        "{} Sending WASM execution request to '{}'...",
        Icons::sync(),
        peer_alias
    ));
}

async fn handle_wasm_config(args: &[&str], ui_logger: &UILogger, error_logger: &ErrorLogger) {
    if args.is_empty() {
        if let Some(config) =
            load_config_or_log(ui_logger, error_logger, "wasm config status").await
        {
            ui_logger.log("WASM Capability Configuration:".to_string());
            ui_logger.log(format!(
                "  Advertise capabilities: {}",
                if config.wasm.capability.advertise_capabilities {
                    "enabled"
                } else {
                    "disabled"
                }
            ));
            ui_logger.log(format!(
                "  Allow remote execution: {}",
                if config.wasm.capability.allow_remote_execution {
                    "enabled"
                } else {
                    "disabled"
                }
            ));
            ui_logger.log(format!(
                "  Max offerings: {}",
                config.wasm.capability.max_offerings
            ));
            ui_logger.log(format!(
                "  Max concurrent executions: {}",
                config.wasm.capability.max_concurrent_executions
            ));
        }
        return;
    }

    let setting = args[0];
    let value = args.get(1).copied().unwrap_or("status");

    match setting {
        "advertise" => match value {
            "on" => {
                if modify_config(ui_logger, error_logger, "wasm-advertise", |config| {
                    config.wasm.capability.advertise_capabilities = true;
                })
                .await
                {
                    ui_logger.log(format!(
                        "{} WASM capability advertisement enabled",
                        Icons::check()
                    ));
                }
            }
            "off" => {
                if modify_config(ui_logger, error_logger, "wasm-advertise", |config| {
                    config.wasm.capability.advertise_capabilities = false;
                })
                .await
                {
                    ui_logger.log(format!(
                        "{} WASM capability advertisement disabled",
                        Icons::check()
                    ));
                }
            }
            _ => {
                if let Some(config) =
                    load_config_or_log(ui_logger, error_logger, "wasm advertise status").await
                {
                    ui_logger.log(format!(
                        "WASM capability advertisement is {}",
                        if config.wasm.capability.advertise_capabilities {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ));
                }
            }
        },
        "execute" => match value {
            "on" => {
                if modify_config(ui_logger, error_logger, "wasm-execute", |config| {
                    config.wasm.capability.allow_remote_execution = true;
                })
                .await
                {
                    ui_logger.log(format!("{} Remote WASM execution enabled", Icons::check()));
                    ui_logger.log(format!(
                        "{} Warning: This allows other peers to execute WASM on your node",
                        Icons::warning()
                    ));
                }
            }
            "off" => {
                if modify_config(ui_logger, error_logger, "wasm-execute", |config| {
                    config.wasm.capability.allow_remote_execution = false;
                })
                .await
                {
                    ui_logger.log(format!("{} Remote WASM execution disabled", Icons::check()));
                }
            }
            _ => {
                if let Some(config) =
                    load_config_or_log(ui_logger, error_logger, "wasm execute status").await
                {
                    ui_logger.log(format!(
                        "Remote WASM execution is {}",
                        if config.wasm.capability.allow_remote_execution {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ));
                }
            }
        },
        _ => {
            ui_logger.usage("wasm config [advertise|execute] [on|off|status]");
        }
    }
}
