use crate::KeyAction;

/// Parse a slash command and return the appropriate action
pub fn parse_slash_command(cmd: String) -> Option<KeyAction> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    match parts[0] {
        "model" => {
            if parts.len() > 1 {
                Some(KeyAction::SlashCommandModel { model: parts[1].to_string() })
            } else {
                Some(KeyAction::SlashCommandModel { model: "list".to_string() })
            }
        }
        "approvals" => {
            if parts.len() > 1 {
                Some(KeyAction::SlashCommandApprovals { mode: parts[1].to_string() })
            } else {
                Some(KeyAction::SlashCommandApprovals { mode: "list".to_string() })
            }
        }
        "verbosity" => {
            if parts.len() > 1 {
                Some(KeyAction::SlashCommandVerbosity { level: parts[1].to_string() })
            } else {
                Some(KeyAction::SlashCommandVerbosity { level: "list".to_string() })
            }
        }
        "status" => Some(KeyAction::SlashCommandStatus),
        "plan" => {
            if parts.len() > 1 {
                match parts[1] {
                    "add" => {
                        if parts.len() > 2 {
                            let item = parts[2..].join(" ");
                            Some(KeyAction::SlashCommandPlanAdd { item })
                        } else {
                            None
                        }
                    }
                    "done" => {
                        if parts.len() > 2 {
                            if let Ok(index) = parts[2].parse::<usize>() {
                                Some(KeyAction::SlashCommandPlanDone { index })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => Some(KeyAction::SlashCommandPlan),
                }
            } else {
                Some(KeyAction::SlashCommandPlan)
            }
        }
        "review" => Some(KeyAction::SlashCommandReview),
        "memory" => {
            if parts.len() > 1 {
                match parts[1] {
                    "add" => {
                        if parts.len() > 2 {
                            let fact = parts[2..].join(" ");
                            Some(KeyAction::SlashCommandMemoryAdd { fact })
                        } else {
                            None
                        }
                    }
                    "search" => {
                        if parts.len() > 2 {
                            let query = parts[2..].join(" ");
                            Some(KeyAction::SlashCommandMemorySearch { query })
                        } else {
                            None
                        }
                    }
                    "pin" => {
                        if parts.len() > 2 {
                            let id = parts[2..].join(" ");
                            Some(KeyAction::SlashCommandMemoryPin { id })
                        } else {
                            None
                        }
                    }
                    _ => Some(KeyAction::SlashCommandMemory),
                }
            } else {
                Some(KeyAction::SlashCommandMemory)
            }
        }
        "clear" => Some(KeyAction::SlashCommandClear),
        "config" => Some(KeyAction::SlashCommandConfig),
        "garden" => {
            if parts.len() > 1 {
                match parts[1] {
                    "consolidate" => {
                        if parts.len() > 2 {
                            Some(KeyAction::SlashCommandGardenConsolidate { session_id: parts[2].to_string() })
                        } else {
                            Some(KeyAction::SlashCommandGardenConsolidate { session_id: "latest".to_string() })
                        }
                    }
                    "hygiene" => Some(KeyAction::SlashCommandGardenHygiene),
                    "drift" => Some(KeyAction::SlashCommandGardenDrift),
                    "verify" => {
                        if parts.len() > 2 {
                            Some(KeyAction::SlashCommandGardenVerify { doc_id: parts[2].to_string() })
                        } else {
                            None
                        }
                    }
                    "stats" => Some(KeyAction::SlashCommandGardenStats),
                    _ => Some(KeyAction::SlashCommandGardenStats),
                }
            } else {
                Some(KeyAction::SlashCommandGardenStats)
            }
        }
        "search" => {
            if parts.len() > 1 {
                let (scope, query_start) = if parts[1] == "--events" {
                    (thunderus_core::SearchScope::Events, 2)
                } else if parts[1] == "--views" {
                    (thunderus_core::SearchScope::Views, 2)
                } else {
                    (thunderus_core::SearchScope::All, 1)
                };

                if parts.len() > query_start {
                    let query = parts[query_start..].join(" ");
                    Some(KeyAction::SlashCommandSearch { query, scope })
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_slash_command_model() {
        let action = parse_slash_command("model glm-4.7".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandModel { .. })));
        if let Some(KeyAction::SlashCommandModel { model }) = action {
            assert_eq!(model, "glm-4.7");
        }
    }

    #[test]
    fn test_parse_slash_command_model_list() {
        let action = parse_slash_command("model".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandModel { .. })));
        if let Some(KeyAction::SlashCommandModel { model }) = action {
            assert_eq!(model, "list");
        }
    }

    #[test]
    fn test_parse_slash_command_approvals() {
        let action = parse_slash_command("approvals read-only".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandApprovals { .. })));
        if let Some(KeyAction::SlashCommandApprovals { mode }) = action {
            assert_eq!(mode, "read-only");
        }
    }

    #[test]
    fn test_parse_slash_command_approvals_list() {
        let action = parse_slash_command("approvals".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandApprovals { .. })));
        if let Some(KeyAction::SlashCommandApprovals { mode }) = action {
            assert_eq!(mode, "list");
        }
    }

    #[test]
    fn test_parse_slash_command_verbosity() {
        let action = parse_slash_command("verbosity verbose".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandVerbosity { .. })));
        if let Some(KeyAction::SlashCommandVerbosity { level }) = action {
            assert_eq!(level, "verbose");
        }
    }

    #[test]
    fn test_parse_slash_command_verbosity_list() {
        let action = parse_slash_command("verbosity".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandVerbosity { .. })));
        if let Some(KeyAction::SlashCommandVerbosity { level }) = action {
            assert_eq!(level, "list");
        }
    }

    #[test]
    fn test_parse_slash_command_verbosity_all_levels() {
        let action_quiet = parse_slash_command("verbosity quiet".to_string());
        if let Some(KeyAction::SlashCommandVerbosity { level }) = action_quiet {
            assert_eq!(level, "quiet");
        }

        let action_default = parse_slash_command("verbosity default".to_string());
        if let Some(KeyAction::SlashCommandVerbosity { level }) = action_default {
            assert_eq!(level, "default");
        }

        let action_verbose = parse_slash_command("verbosity verbose".to_string());
        if let Some(KeyAction::SlashCommandVerbosity { level }) = action_verbose {
            assert_eq!(level, "verbose");
        }
    }

    #[test]
    fn test_parse_slash_command_status() {
        let action = parse_slash_command("status".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandStatus)));
    }

    #[test]
    fn test_parse_slash_command_plan() {
        let action = parse_slash_command("plan".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandPlan)));
    }

    #[test]
    fn test_parse_slash_command_review() {
        let action = parse_slash_command("review".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandReview)));
    }

    #[test]
    fn test_parse_slash_command_memory() {
        let action = parse_slash_command("memory".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandMemory)));
    }

    #[test]
    fn test_parse_slash_command_clear() {
        let action = parse_slash_command("clear".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandClear)));
    }

    #[test]
    fn test_parse_slash_command_config() {
        let action = parse_slash_command("config".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandConfig)));
    }

    #[test]
    fn test_parse_slash_command_unknown() {
        let action = parse_slash_command("unknown_command".to_string());
        assert!(action.is_none());
    }

    #[test]
    fn test_parse_slash_command_empty() {
        let action = parse_slash_command("".to_string());
        assert!(action.is_none());
    }

    #[test]
    fn test_parse_slash_command_whitespace_only() {
        let action = parse_slash_command("   ".to_string());
        assert!(action.is_none());
    }

    #[test]
    fn test_parse_slash_command_extra_whitespace() {
        let action = parse_slash_command("  model   glm-4.7  ".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandModel { .. })));
        if let Some(KeyAction::SlashCommandModel { model }) = action {
            assert_eq!(model, "glm-4.7");
        }
    }

    #[test]
    fn test_parse_slash_command_multiple_words() {
        let action = parse_slash_command("model some model name".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandModel { .. })));
        if let Some(KeyAction::SlashCommandModel { model }) = action {
            assert_eq!(model, "some");
        }
    }
}
