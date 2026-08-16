//! App-to-view projections that need access to runtime-owned state.

use super::*;

impl App {
    pub(super) fn render_queued_summary_view(&self) -> Option<QueuedSummaryView> {
        let steering_count = self.composer.queue.pending_count(QueueTarget::Steering);
        let followup_count = self.composer.queue.pending_count(QueueTarget::FollowUp);
        if steering_count == 0 && followup_count == 0 {
            None
        } else {
            Some(QueuedSummaryView {
                steering_count,
                followup_count,
                target: self.composer.queue_target.label().to_string(),
            })
        }
    }

    pub(super) fn render_prompt_suggestions(&self) -> Vec<PromptSuggestionView> {
        match self.overlay.accessory() {
            PromptAccessory::Commands { selected } => crate::app::command_suggestions_for_app(self)
                .into_iter()
                .enumerate()
                .map(|(index, suggestion)| PromptSuggestionView {
                    label: suggestion.name,
                    detail: suggestion.detail,
                    selected: index == selected,
                    kind: PromptSuggestionKind::Command,
                })
                .collect(),
            PromptAccessory::Files(FilePickerSource::Mention { .. }) => {
                self.render_picker_suggestions(PromptSuggestionKind::FileMention)
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn render_picker_suggestions(&self, kind: PromptSuggestionKind) -> Vec<PromptSuggestionView> {
        self.overlay
            .picker()
            .map(|picker| {
                picker
                    .matches
                    .iter()
                    .enumerate()
                    .map(|(index, item)| PromptSuggestionView {
                        label: item.label.clone(),
                        detail: item.detail.clone(),
                        selected: index == picker.selected,
                        kind,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn render_picker_surface(&self, title: &str) -> Option<PickerView> {
        let picker = self.overlay.picker()?;
        Some(PickerView {
            title: title.to_string(),
            query: picker.query.clone(),
            selected: picker.selected,
            items: picker
                .matches
                .iter()
                .map(|item| PickerItemView { label: item.label.clone(), detail: item.detail.clone() })
                .collect(),
        })
    }

    pub(super) fn render_setup_form_view(&self) -> Option<SetupFormView> {
        let recovery = self.overlay.setup()?;
        let (label, value, secret) = setup_field(recovery);
        let provider = recovery
            .provider
            .map(|provider| provider.label().to_string())
            .unwrap_or_else(|| "advanced / ACP".to_string());
        let stage = recovery.stage.label().to_string();
        let step = match (recovery.intent, recovery.stage) {
            (crate::app::RecoveryIntent::Reauthenticate, RecoveryStage::MissingCredential) => "session rejected",
            (crate::app::RecoveryIntent::Reauthenticate, RecoveryStage::EnterKey) => "replace API key",
            (crate::app::RecoveryIntent::Reauthenticate, RecoveryStage::EnvironmentCredentialRejected) => {
                "restart required"
            }
            (_, RecoveryStage::MissingCredential) if recovery.provider == Some(SetupProviderArg::ChatgptCodex) => {
                "connect ChatGPT"
            }
            (_, RecoveryStage::MissingCredential) => "add API key",
            (_, RecoveryStage::EnterKey) => "enter API key",
            _ => stage.as_str(),
        };
        let status = format!("{provider} · {step}");
        let details = setup_details(recovery);
        let fields = if matches!(
            recovery.stage,
            RecoveryStage::EnterKey | RecoveryStage::ChatGptOAuthPasteRedirect
        ) {
            Vec::new()
        } else {
            vec![SetupFieldView {
                label,
                value,
                focused: recovery.action_count() == 0,
                secret,
                multiline: false,
                error: None,
            }]
        };
        Some(SetupFormView {
            title: recovery.intent.label().to_string(),
            attention: recovery.intent == crate::app::RecoveryIntent::Reauthenticate,
            stage,
            status,
            details,
            fields,
            focus_index: 0,
            actions: setup_actions(recovery),
            selected: recovery.selected,
            validation_errors: Vec::new(),
            submit_label: if recovery.stage == RecoveryStage::EnterKey {
                "submit".to_string()
            } else {
                "continue".to_string()
            },
            cancel_label: setup_cancel_label(recovery).to_string(),
            complete: false,
        })
    }

    /// Project the context ledger into bounded table data owned by the renderer.
    pub fn render_context_table(&self) -> TableView {
        let Some(ledger) = &self.transcript.context_ledger else {
            return TableView {
                header: vec![TableCellView {
                    text: "context".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Flexible,
                }],
                rows: vec![vec![TableCellView {
                    text: "no ledger".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Flexible,
                }]],
                selected_row: None,
                narrow_fallback: vec!["context unavailable".to_string()],
            };
        };

        let review = self
            .transcript
            .last_compaction_review
            .map(|a| a.label())
            .unwrap_or("none");
        let projection = ledger.projection();
        let remaining = projection
            .remaining_percent
            .map_or_else(|| "unknown".to_string(), |value| format!("{value}%"));
        let mut rows = vec![
            context_table_row(
                "next request",
                &format!("{} / {}", projection.used, projection.available_input),
                &remaining,
                "projected input",
            ),
            context_table_row(
                "thresholds",
                &format!(
                    "target {} / compact {}",
                    projection.target, projection.auto_compaction_threshold
                ),
                "tokens",
                &projection.estimate_provenance,
            ),
            context_table_row(
                "model limits",
                projection.limit_source.label(),
                projection.limit_confidence.label(),
                "source / confidence",
            ),
            context_table_row(
                "items",
                &format!("selected {} / omitted {}", projection.selected, projection.omitted),
                &format!("recoverable {}", projection.recoverable),
                &format!("protected {}", projection.protected),
            ),
        ];
        rows.extend(projection.categories.iter().map(|total| {
            context_table_row(
                total.category.label(),
                &format!("{} / {}", total.selected_tokens, total.available_tokens),
                &format!("{} / {} items", total.selected_items, total.available_items),
                "selected / available",
            )
        }));
        rows.push(context_table_row(
            "compaction",
            &format!("{} / {}", self.effective_compaction_policy().mode.label(), review),
            "state",
            "review",
        ));
        rows.extend(ledger.diagnostics.iter().map(|diagnostic| {
            context_table_row(
                "diagnostic",
                &diagnostic.code,
                diagnostic.severity.label(),
                &diagnostic.message,
            )
        }));
        rows.extend(
            ledger
                .items
                .iter()
                .take(crate::app::CONTEXT_INSPECTION_MAX_ITEMS)
                .map(|item| {
                    let details = crate::context::export::export_item(item);
                    vec![
                        TableCellView {
                            text: redact_context_display(&item.id),
                            alignment: ColumnAlignment::Left,
                            width: ColumnWidthPolicy::Percent(34),
                        },
                        TableCellView {
                            text: format!(
                                "{} / {} lifecycle:{} reason:{} prot:{} [{}] rec:{} repl:{} verify:{}",
                                item.kind.label(),
                                item.visibility.label(),
                                details.lifecycle.label(),
                                details.reason_code,
                                yes_no(details.protected),
                                context_protection_label(&details),
                                yes_no(details.recovery_available),
                                details.replacement.as_deref().unwrap_or("none"),
                                details.verification.as_deref().unwrap_or("none")
                            ),
                            alignment: ColumnAlignment::Left,
                            width: ColumnWidthPolicy::Percent(26),
                        },
                        TableCellView {
                            text: item.token_estimate.to_string(),
                            alignment: ColumnAlignment::Right,
                            width: ColumnWidthPolicy::Fixed(9),
                        },
                        TableCellView {
                            text: redact_context_display(&item.label),
                            alignment: ColumnAlignment::Left,
                            width: ColumnWidthPolicy::Flexible,
                        },
                    ]
                }),
        );

        let mut narrow_fallback = vec![
            format!(
                "next request {} / {} tokens, {} remaining",
                projection.used, projection.available_input, remaining
            ),
            format!(
                "target {} compact {} estimate {}",
                projection.target, projection.auto_compaction_threshold, projection.estimate_provenance
            ),
            format!(
                "limits {} ({})",
                projection.limit_source.label(),
                projection.limit_confidence.label()
            ),
            format!(
                "compaction {} review {}",
                self.effective_compaction_policy().mode.label(),
                review
            ),
            format!(
                "items selected {} omitted {} recoverable {} protected {}",
                projection.selected, projection.omitted, projection.recoverable, projection.protected
            ),
        ];
        narrow_fallback.extend(projection.categories.iter().map(|total| {
            format!(
                "{} {} / {} tokens ({} / {} items)",
                total.category.label(),
                total.selected_tokens,
                total.available_tokens,
                total.selected_items,
                total.available_items
            )
        }));
        narrow_fallback.extend(ledger.diagnostics.iter().map(|diagnostic| diagnostic.summary()));
        narrow_fallback.extend(ledger.items.iter().take(CONTEXT_INSPECTION_MAX_ITEMS).map(|item| {
            let details = crate::context::export::export_item(item);
            format!(
                "{} visibility {} lifecycle {} reason {} protected {} [{}] recovery {} replacement {} relations {}",
                redact_context_display(&item.id),
                item.visibility.label(),
                details.lifecycle.label(),
                details.reason_code,
                yes_no(details.protected),
                context_protection_label(&details),
                yes_no(details.recovery_available),
                details.replacement.as_deref().unwrap_or("none"),
                details
                    .relations
                    .iter()
                    .map(|relation| {
                        format!(
                            "{}:{}->{}:{}",
                            relation.kind.label(),
                            redact_context_display(&relation.id),
                            redact_context_display(&relation.target_id),
                            relation.status.label()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }));
        TableView {
            header: vec![
                TableCellView {
                    text: "context".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Percent(34),
                },
                TableCellView {
                    text: "visibility / lifecycle / protection / relations".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Percent(26),
                },
                TableCellView {
                    text: "tokens".to_string(),
                    alignment: ColumnAlignment::Right,
                    width: ColumnWidthPolicy::Fixed(9),
                },
                TableCellView {
                    text: "label".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Flexible,
                },
            ],
            rows,
            selected_row: None,
            narrow_fallback,
        }
    }
}

fn setup_details(recovery: &FirstRunRecovery) -> Vec<String> {
    let mut details = Vec::new();
    match recovery.stage {
        RecoveryStage::ChooseProvider => {
            details.push("Choose a provider before a model; no provider or model is assumed by setup.".to_string())
        }
        RecoveryStage::ModelSelection => details.push("Choose the model available for this provider.".to_string()),
        RecoveryStage::ModelConfigScope => {
            details.push("Optionally save the selected model to project or global config.".to_string())
        }
        RecoveryStage::UnsupportedRoute => {
            details.push(crate::cli::commands::setup::UNSUPPORTED_PROVIDER_ROUTE_MESSAGE.to_string())
        }
        RecoveryStage::MissingCredential => match (recovery.intent, recovery.provider) {
            (crate::app::RecoveryIntent::Reauthenticate, Some(SetupProviderArg::ChatgptCodex)) => details.push(
                "Session rejected. Sign in again in your browser, or use device code on a headless machine. Your draft is preserved."
                    .to_string(),
            ),
            (_, Some(SetupProviderArg::ChatgptCodex)) => details.push(
                "Browser PKCE is the default. Device code is an explicit headless route; neither asks for an API key."
                    .to_string(),
            ),
            _ => details
                .push("The credential stays hidden and is written only after an explicit scope choice.".to_string()),
        },
        RecoveryStage::EnterKey => {
            if recovery.intent == crate::app::RecoveryIntent::Reauthenticate {
                details.push("Key rejected. Enter a replacement; your draft is preserved.".to_string());
            } else {
                details.push("Input is hidden. Enter continues; Esc preserves the draft.".to_string());
            }
        }
        RecoveryStage::ConfirmStore => details.push("Choose where the credential may be stored.".to_string()),
        RecoveryStage::Instructions => details.push(setup_instruction(recovery).to_string()),
        RecoveryStage::ChatGptOAuthRequesting => {
            details.push("Starting the selected ChatGPT OAuth method.".to_string())
        }
        RecoveryStage::ChatGptOAuthPolling => match recovery.chatgpt_oauth.as_ref() {
            Some(oauth) => {
                match oauth.method {
                    ChatGptOAuthMethod::Browser => {
                        details.push("Open or copy this authorization URL:".to_string());
                        if let Some(url) = oauth.authorization_url.as_deref() {
                            details.push(url.to_string());
                        }
                    }
                    _ => {
                        if let Some(code) = oauth.code.as_ref() {
                            let uri = code
                                .verification_uri
                                .as_deref()
                                .unwrap_or("https://auth.openai.com/codex/device");
                            details.push(format!("Open {uri} and enter code {}.", code.user_code));
                        }
                    }
                };
                details.push(oauth.status.clone());
            }
            None => details.push("Waiting for ChatGPT OAuth.".to_string()),
        },
        RecoveryStage::ChatGptOAuthPasteRedirect => {
            details.push("Paste the full browser redirect URL. Input is hidden.".to_string())
        }
        RecoveryStage::ChatGptOAuthFailed => details.push(
            recovery
                .chatgpt_oauth
                .as_ref()
                .map(|oauth| oauth.status.clone())
                .unwrap_or_else(|| "ChatGPT OAuth failed.".to_string()),
        ),
        RecoveryStage::EnvironmentCredentialRejected => {
            let env_var = recovery
                .provider
                .and_then(|provider| match provider {
                    SetupProviderArg::ChatgptCodex => Some(crate::thndrs_core::auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV),
                    _ => provider.api_key_env_var(),
                })
                .unwrap_or("environment variable");
            details.push(format!(
                "{env_var} was rejected. Replace or unset it, then restart thndrs. Stored credentials cannot override it; your draft is preserved."
            ));
        }
        RecoveryStage::LogoutConfirm => details.push("Remove the credential from the selected store.".to_string()),
        RecoveryStage::AcpMissing => {
            details.push("ACP models use ACP agent config, not provider API keys.".to_string())
        }
    }
    details
}

fn setup_actions(recovery: &FirstRunRecovery) -> Vec<PickerItemView> {
    let incomplete_setup_action =
        if recovery.pending_provider_prompt { "return to draft" } else { "continue without setup" };
    let labels: Vec<String> = match recovery.stage {
        RecoveryStage::ChooseProvider => vec![
            "ChatGPT Codex".to_string(),
            "OpenCode Zen".to_string(),
            "OpenCode Go".to_string(),
            "show setup instructions".to_string(),
        ],
        RecoveryStage::UnsupportedRoute => vec!["switch provider/model".to_string(), "quit".to_string()],
        RecoveryStage::ModelSelection => recovery
            .provider
            .map(crate::app::setup_model_options)
            .unwrap_or_default()
            .into_iter()
            .map(|item| item.label)
            .collect(),
        RecoveryStage::ModelConfigScope => vec![
            "project config".to_string(),
            "global config".to_string(),
            "skip model config".to_string(),
            "cancel setup".to_string(),
        ],
        RecoveryStage::MissingCredential => {
            if recovery.provider == Some(SetupProviderArg::ChatgptCodex) {
                vec![
                    "start browser PKCE login".to_string(),
                    "use headless device code".to_string(),
                    "switch model/provider".to_string(),
                    "show setup instructions".to_string(),
                    incomplete_setup_action.to_string(),
                    "quit".to_string(),
                ]
            } else {
                vec![
                    "enter API key".to_string(),
                    "switch model/provider".to_string(),
                    "show setup instructions".to_string(),
                    incomplete_setup_action.to_string(),
                    "quit".to_string(),
                ]
            }
        }
        RecoveryStage::EnterKey => Vec::new(),
        RecoveryStage::ConfirmStore | RecoveryStage::LogoutConfirm => vec![
            "global credentials".to_string(),
            "project credentials".to_string(),
            "cancel".to_string(),
        ],
        RecoveryStage::Instructions => vec!["back".to_string(), "close".to_string()],
        RecoveryStage::ChatGptOAuthRequesting | RecoveryStage::ChatGptOAuthPasteRedirect => vec!["cancel".to_string()],
        RecoveryStage::ChatGptOAuthPolling => {
            if recovery
                .chatgpt_oauth
                .as_ref()
                .is_some_and(|oauth| oauth.method == ChatGptOAuthMethod::Browser)
            {
                vec!["cancel".to_string(), "paste full redirect URL".to_string()]
            } else {
                vec!["cancel".to_string()]
            }
        }
        RecoveryStage::ChatGptOAuthFailed => vec![
            "retry browser PKCE".to_string(),
            "use headless device code".to_string(),
            "back".to_string(),
        ],
        RecoveryStage::EnvironmentCredentialRejected => vec![
            "switch model/provider".to_string(),
            "close".to_string(),
            "quit".to_string(),
        ],
        RecoveryStage::AcpMissing => vec![
            "switch model/provider".to_string(),
            "show ACP setup".to_string(),
            incomplete_setup_action.to_string(),
            "quit".to_string(),
        ],
    };
    labels
        .into_iter()
        .map(|label| PickerItemView { detail: String::new(), label })
        .collect()
}

fn setup_instruction(recovery: &FirstRunRecovery) -> &'static str {
    match recovery.provider {
        Some(SetupProviderArg::ChatgptCodex) => {
            "Run `thndrs setup --provider chatgpt-codex` or `thndrs login chatgpt-codex` outside the TUI."
        }
        Some(SetupProviderArg::Umans) => crate::cli::commands::setup::UNSUPPORTED_PROVIDER_ROUTE_MESSAGE,
        Some(_) => "Run `thndrs setup` or `thndrs login <provider>` outside the TUI.",

        None => "Advanced providers remain available through `thndrs setup` or ACP configuration.",
    }
}

fn setup_field(recovery: &FirstRunRecovery) -> (String, String, bool) {
    match recovery.stage {
        RecoveryStage::ChooseProvider => ("provider".to_string(), "choose provider".to_string(), false),
        RecoveryStage::UnsupportedRoute => (
            "provider".to_string(),
            "choose a supported provider or model".to_string(),
            false,
        ),
        RecoveryStage::ModelSelection => (
            "model".to_string(),
            recovery
                .provider
                .map(crate::app::setup_model_options)
                .and_then(|options| options.get(recovery.selected).map(|item| item.label.clone()))
                .unwrap_or_else(|| "choose model".to_string()),
            false,
        ),
        RecoveryStage::ModelConfigScope => (
            "config".to_string(),
            match recovery.selected {
                0 => "project config".to_string(),
                1 => "global config".to_string(),
                2 => "skip model config".to_string(),
                _ => "cancel setup".to_string(),
            },
            false,
        ),
        RecoveryStage::EnterKey => (
            recovery
                .provider
                .map(|provider| format!("{} API key", provider.label()))
                .unwrap_or_else(|| "API key".to_string()),
            if recovery.secret_input.is_empty() { String::new() } else { "[hidden]".to_string() },
            true,
        ),
        RecoveryStage::MissingCredential => (
            "provider".to_string(),
            recovery
                .provider
                .map_or_else(|| "advanced / ACP".to_string(), |provider| provider.label().to_string()),
            false,
        ),
        RecoveryStage::ConfirmStore => (
            "credential scope".to_string(),
            match recovery.selected {
                0 => "global credentials".to_string(),
                1 => "project credentials".to_string(),
                _ => "cancel".to_string(),
            },
            false,
        ),
        RecoveryStage::Instructions => ("next".to_string(), "follow setup instructions".to_string(), false),
        RecoveryStage::ChatGptOAuthRequesting | RecoveryStage::ChatGptOAuthPolling => {
            ("provider".to_string(), "ChatGPT OAuth".to_string(), false)
        }
        RecoveryStage::ChatGptOAuthPasteRedirect => (
            "redirect URL".to_string(),
            if recovery.secret_input.is_empty() { String::new() } else { "[hidden]".to_string() },
            true,
        ),
        RecoveryStage::ChatGptOAuthFailed => ("provider".to_string(), "ChatGPT OAuth failed".to_string(), false),
        RecoveryStage::EnvironmentCredentialRejected => (
            "credential source".to_string(),
            recovery
                .provider
                .and_then(|provider| match provider {
                    SetupProviderArg::ChatgptCodex => Some(crate::thndrs_core::auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV),
                    _ => provider.api_key_env_var(),
                })
                .unwrap_or("environment variable")
                .to_string(),
            false,
        ),
        RecoveryStage::LogoutConfirm => (
            "credential scope".to_string(),
            match recovery.selected {
                0 => "global credentials",
                1 => "project credentials",
                _ => "cancel",
            }
            .to_string(),
            false,
        ),
        RecoveryStage::AcpMissing => ("provider".to_string(), "ACP agent config".to_string(), false),
    }
}

fn setup_cancel_label(recovery: &FirstRunRecovery) -> &'static str {
    match recovery.stage {
        RecoveryStage::EnterKey
        | RecoveryStage::ChatGptOAuthRequesting
        | RecoveryStage::ChatGptOAuthPolling
        | RecoveryStage::ChatGptOAuthPasteRedirect => "back",
        _ => "close",
    }
}

fn context_table_row(name: &str, state: &str, tokens: &str, label: &str) -> Vec<TableCellView> {
    vec![
        TableCellView {
            text: name.to_string(),
            alignment: ColumnAlignment::Left,
            width: ColumnWidthPolicy::Percent(34),
        },
        TableCellView {
            text: state.to_string(),
            alignment: ColumnAlignment::Left,
            width: ColumnWidthPolicy::Percent(26),
        },
        TableCellView {
            text: tokens.to_string(),
            alignment: ColumnAlignment::Right,
            width: ColumnWidthPolicy::Fixed(9),
        },
        TableCellView { text: label.to_string(), alignment: ColumnAlignment::Left, width: ColumnWidthPolicy::Flexible },
    ]
}

fn redact_context_display(value: &str) -> String {
    utils::truncate_ellipsis(&redact_secrets(value), 160)
}

fn context_protection_label(item: &crate::context::export::ExportContextItem) -> String {
    if item.protection_released {
        return "released".to_string();
    }
    let labels = item.protection.labels();
    if labels.is_empty() { "none".to_string() } else { labels.join(",") }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
