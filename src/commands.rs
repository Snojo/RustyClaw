use crate::secrets::SecretsManager;
use crate::skills::SkillManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAction {
    None,
    ClearMessages,
    Quit,
    /// Start (connect) the gateway
    GatewayStart,
    /// Stop (disconnect) the gateway
    GatewayStop,
    /// Restart the gateway connection
    GatewayRestart,
    /// Show gateway status info (no subcommand given)
    GatewayInfo,
}

#[derive(Debug, Clone)]
pub struct CommandResponse {
    pub messages: Vec<String>,
    pub action: CommandAction,
}

pub struct CommandContext<'a> {
    pub secrets_manager: &'a mut SecretsManager,
    pub skill_manager: &'a mut SkillManager,
}

/// List of all known command names (without the / prefix).
/// Includes subcommand forms so tab-completion works for them.
pub const COMMAND_NAMES: &[&str] = &[
    "help",
    "clear",
    "enable-access",
    "disable-access",
    "reload-skills",
    "gateway",
    "gateway start",
    "gateway stop",
    "gateway restart",
    "quit",
];

pub fn handle_command(input: &str, context: &mut CommandContext<'_>) -> CommandResponse {
    // Strip the leading '/' if present
    let trimmed = input.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return CommandResponse {
            messages: Vec::new(),
            action: CommandAction::None,
        };
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return CommandResponse {
            messages: Vec::new(),
            action: CommandAction::None,
        };
    }

    match parts[0] {
        "help" => CommandResponse {
            messages: vec![
                "Available commands:".to_string(),
                "  /help                    - Show this help".to_string(),
                "  /clear                   - Clear messages".to_string(),
                "  /enable-access           - Enable agent access to secrets".to_string(),
                "  /disable-access          - Disable agent access to secrets".to_string(),
                "  /reload-skills           - Reload skills".to_string(),
                "  /gateway                 - Show gateway connection status".to_string(),
                "  /gateway start           - Connect to the gateway".to_string(),
                "  /gateway stop            - Disconnect from the gateway".to_string(),
                "  /gateway restart         - Restart the gateway connection".to_string(),
                "  /quit                    - Quit".to_string(),
            ],
            action: CommandAction::None,
        },
        "clear" => CommandResponse {
            messages: vec!["Messages cleared.".to_string()],
            action: CommandAction::ClearMessages,
        },
        "enable-access" => {
            context.secrets_manager.set_agent_access(true);
            CommandResponse {
                messages: vec!["Agent access to secrets enabled.".to_string()],
                action: CommandAction::None,
            }
        }
        "disable-access" => {
            context.secrets_manager.set_agent_access(false);
            CommandResponse {
                messages: vec!["Agent access to secrets disabled.".to_string()],
                action: CommandAction::None,
            }
        }
        "reload-skills" => match context.skill_manager.load_skills() {
            Ok(_) => CommandResponse {
                messages: vec![format!(
                    "Reloaded {} skills.",
                    context.skill_manager.get_skills().len()
                )],
                action: CommandAction::None,
            },
            Err(err) => CommandResponse {
                messages: vec![format!("Error reloading skills: {}", err)],
                action: CommandAction::None,
            },
        },
        "gateway" => match parts.get(1).copied() {
            Some("start") => CommandResponse {
                messages: vec!["Starting gateway connection…".to_string()],
                action: CommandAction::GatewayStart,
            },
            Some("stop") => CommandResponse {
                messages: vec!["Stopping gateway connection…".to_string()],
                action: CommandAction::GatewayStop,
            },
            Some("restart") => CommandResponse {
                messages: vec!["Restarting gateway connection…".to_string()],
                action: CommandAction::GatewayRestart,
            },
            Some(sub) => CommandResponse {
                messages: vec![
                    format!("Unknown gateway subcommand: {}", sub),
                    "Usage: /gateway start|stop|restart".to_string(),
                ],
                action: CommandAction::None,
            },
            None => CommandResponse {
                messages: Vec::new(),
                action: CommandAction::GatewayInfo,
            },
        },
        "q" | "quit" | "exit" => CommandResponse {
            messages: Vec::new(),
            action: CommandAction::Quit,
        },
        _ => CommandResponse {
            messages: vec![
                format!("Unknown command: /{}", parts[0]),
                "Type /help for available commands".to_string(),
            ],
            action: CommandAction::None,
        },
    }
}
