//! Interactive event, effect, and agent lifecycle loop.

use super::*;

/// Interactive alternate-screen mode with one application-owned viewport.
pub(crate) fn run_inline(tick: Duration, cli: &Cli, initial_session: InitialSession<'_>) -> io::Result<()> {
    let app = match initial_session {
        InitialSession::New => App::from_cli(cli),
        InitialSession::Resume(session_id) => App::from_cli_resuming(cli, session_id)?,
    };
    let mouse_enabled = cli.mouse && !cli.no_mouse;
    let terminal_session = AlternateScreenSession::enter(mouse_enabled)?;
    let stdout = io::BufWriter::with_capacity(TERMINAL_WRITE_BUFFER_CAPACITY, io::stdout());
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let mut surface = RatatuiSurface::new(terminal, terminal_session);
    let resume_message = interactive_loop(&mut surface, tick, cli, app)?;
    surface.terminal_session.suspend()?;
    drop(surface);
    if let Some(message) = resume_message {
        println!("{message}");
    }
    Ok(())
}

/// Renderer-neutral interactive coordinator for application, agent, terminal,
/// and render events.
pub(crate) fn interactive_loop<S: InteractiveSurface>(
    surface: &mut S, tick: Duration, cli: &Cli, mut app: App,
) -> io::Result<Option<String>> {
    let tick = tick.max(MIN_RENDER_INTERVAL);
    let workspace_root = crate::context::discover_workspace_root(&cli.cwd);
    let observability = init_tracing(&workspace_root, &app.session.id, app.session.run_persistence);
    if cli.verbose
        && let Some(obs) = &observability
    {
        app.transcript
            .entries
            .push(app::Entry::Status { text: format!("logs  {}", obs.session_log_path.display()) });
    }
    tracing::info!(
        session = %app.session.id,
        cwd = %workspace_root.display(),
        model = %cli.model,
        websearch = %cli.websearch.label(),
        "starting thndrs (ratatui renderer)"
    );
    append_daily_log(
        &observability,
        &app.session.id,
        "session_start",
        &format!(
            "cwd={} model={} websearch={}",
            workspace_root.display(),
            cli.model,
            cli.websearch.label()
        ),
    );

    let mut agent: Option<AgentSlot> = None;
    let git_watcher = GitStatusWatcher::spawn(workspace_root);
    let mut presenter = PresentationScheduler::new(tick);
    presenter.request_immediate();
    present_if_due(surface, &mut app, &mut presenter, Instant::now())?;
    let mut next_tick = Instant::now() + tick;

    loop {
        let now = Instant::now();
        if drain_agent_events(&mut app, &mut agent, surface, &observability)? {
            presenter.request_throttled(now);
        }
        if drain_git_status_watcher(&mut app, &git_watcher, surface)? {
            presenter.request_throttled(now);
        }
        flush_steering(&mut app, &agent);

        if now >= next_tick {
            let run_state_before_tick = app.runtime.run_state.clone();
            let had_status_toast = app.runtime.status_toast.is_some();
            handle_msg(&mut app, Msg::Tick, surface, &mut agent)?;
            if tick_requires_render(&run_state_before_tick, had_status_toast, &app) {
                presenter.request_throttled(now);
            }
            next_tick = now + tick;
        }

        present_if_due(surface, &mut app, &mut presenter, now)?;
        if app.runtime.quit {
            tracing::info!("quitting thndrs");
            append_daily_log(&observability, &app.session.id, "session_end", "reason=quit");
            return Ok(session_resume_message(&app));
        }

        let wait_deadline = presenter
            .next_deadline()
            .map_or(next_tick, |deadline| deadline.min(next_tick));
        if !event::poll(wait_deadline.saturating_duration_since(Instant::now()))? {
            continue;
        }
        let Some(terminal_input) = TerminalInput::from_event(event::read()?) else {
            continue;
        };
        let actions = translate_input(&app, terminal_input);
        for action in actions {
            if surface.handle_navigation(&mut app, &action) {
                presenter.request_immediate();
                continue;
            }
            match action {
                Action::Resize { width, height } => {
                    surface.resize(width, height)?;
                    // Ratatui's resize operation clears the viewport for the next draw.
                    presenter.request_immediate();
                }
                Action::Suspend => {
                    handle_msg(&mut app, Msg::Action(Action::Suspend), surface, &mut agent)?;
                    presenter.request_full_repaint();
                }
                action => {
                    handle_msg(&mut app, Msg::Action(action), surface, &mut agent)?;
                    presenter.request_immediate();
                }
            }
        }
        flush_steering(&mut app, &agent);
        present_if_due(surface, &mut app, &mut presenter, Instant::now())?;
    }
}

pub(crate) fn present_if_due<S: InteractiveSurface>(
    surface: &mut S, app: &mut App, presenter: &mut PresentationScheduler, now: Instant,
) -> io::Result<()> {
    if presenter.should_present(now) {
        surface.draw(app, presenter.full_repaint_required())?;
        presenter.mark_presented(Instant::now());
    }
    Ok(())
}

pub(crate) fn tick_requires_render(previous_state: &RunState, had_status_toast: bool, app: &App) -> bool {
    previous_state != &app.runtime.run_state
        || had_status_toast != app.runtime.status_toast.is_some()
        || app.runtime.run_state != RunState::Idle
        || app.overlay.setup().is_some()
        || app.runtime.ctrl_d_pending.is_some()
        || app.runtime.status_toast.is_some()
}

/// Process a message and all pure follow-ups through one application path.
pub(crate) fn handle_msg<S: InteractiveSurface>(
    app: &mut App, msg: Msg, surface: &mut S, agent: &mut Option<AgentSlot>,
) -> io::Result<()> {
    let mut next = Some(msg);
    while let Some(m) = next {
        let result = update_with_effects(app, &m);
        next = result.follow_up;
        for effect in result.effects {
            if let Some(completion) = execute_effect(app, agent, surface, effect)? {
                next = Some(completion);
            }
        }
        if app.runtime.quit && next.is_none() {
            break;
        }
    }
    Ok(())
}

/// Execute one concrete application effect and return its semantic completion.
pub(crate) fn execute_effect<S: InteractiveSurface>(
    app: &mut App, agent: &mut Option<AgentSlot>, surface: &mut S, effect: Effect,
) -> io::Result<Option<Msg>> {
    match effect {
        Effect::StartAgent(request) => {
            spawn_agent(app, agent, request);
            Ok(None)
        }
        Effect::CancelAgent(request) => {
            if agent.as_ref().is_some_and(|slot| slot.request == request)
                && let Some(slot) = agent.as_ref()
            {
                slot.cancel.cancel();
            }
            Ok(None)
        }
        Effect::SettleAgent(request) => {
            settle_agent(agent, &request, app.runtime.stopping_timed_out);
            Ok(None)
        }
        Effect::DrainBackgroundProcesses => {
            let results = app.runtime.process_registry.drain_completed();
            Ok((!results.is_empty()).then(|| Msg::Effect(EffectResult::BackgroundProcesses(results))))
        }
        Effect::ShutdownProcesses => {
            let results = app.runtime.process_registry.shutdown();
            Ok((!results.is_empty()).then(|| Msg::Effect(EffectResult::BackgroundProcesses(results))))
        }
        Effect::ClearTerminal => Ok(surface.clear().err().map(|error| {
            Msg::Effect(EffectResult::Failed {
                request: app.runtime.active_effect_request.clone(),
                operation: "clear terminal",
                error: error.to_string(),
            })
        })),
        Effect::SuspendTerminal => Ok(surface.suspend().err().map(|error| {
            Msg::Effect(EffectResult::Failed {
                request: app.runtime.active_effect_request.clone(),
                operation: "suspend terminal",
                error: error.to_string(),
            })
        })),
    }
}

/// Drain a bounded burst of agent events through the shared update path.
pub(crate) fn drain_agent_events<S: InteractiveSurface>(
    app: &mut App, agent: &mut Option<AgentSlot>, surface: &mut S, observability: &Option<Observability>,
) -> io::Result<bool> {
    let mut changed = false;

    for _ in 0..MAX_AGENT_EVENTS_PER_RENDER {
        let Some(slot) = agent.as_mut() else {
            break;
        };
        let request = slot.request.clone();
        if app.runtime.active_effect_request.is_none() {
            app.runtime.active_effect_request = Some(request.clone());
        }
        let received = slot.receiver.try_recv();
        match received {
            Ok(event) => {
                match &event {
                    app::AgentEvent::Failed(msg) => {
                        tracing::error!(error = %msg, "agent failed");
                        append_daily_log(
                            observability,
                            &app.session.id,
                            "agent_failed",
                            &format!("error={}", daily_detail_value(msg)),
                        );
                    }
                    app::AgentEvent::Cancelled => {
                        tracing::warn!("agent cancelled");
                        append_daily_log(observability, &app.session.id, "agent_cancelled", "");
                    }
                    app::AgentEvent::Finished => {
                        tracing::info!("agent finished");
                        append_daily_log(observability, &app.session.id, "agent_finished", "");
                    }
                    _ => {}
                }
                handle_msg(app, Msg::Effect(EffectResult::Agent { request, event }), surface, agent)?;
                changed = true;
            }
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                let worker_result = agent.take().expect("disconnected slot remains present").receiver.wait();
                if let Err(error) = worker_result {
                    handle_msg(
                        app,
                        Msg::Effect(EffectResult::Agent {
                            request,
                            event: app::AgentEvent::Failed(format!("agent worker failed: {error}")),
                        }),
                        surface,
                        agent,
                    )?;
                    changed = true;
                } else if app.runtime.run_state == RunState::Stopping {
                    handle_msg(
                        app,
                        Msg::Effect(EffectResult::Agent { request, event: app::AgentEvent::Cancelled }),
                        surface,
                        agent,
                    )?;
                    changed = true;
                }
                break;
            }
        }
    }
    Ok(changed)
}

pub(crate) fn drain_git_status_watcher<S: InteractiveSurface>(
    app: &mut App, watcher: &GitStatusWatcher, _surface: &mut S,
) -> io::Result<bool> {
    let mut changed = false;
    while let Ok(status) = watcher.receiver.try_recv() {
        let _ = update_with_effects(app, &Msg::GitStatusChanged(status));
        changed = true;
    }
    Ok(changed)
}

/// Spawn the unified agent stream if the app is in [`RunState::Working`] state
/// and no agent slot exists yet.
///
/// The run chooses a provider from the selected model id. The
/// [`agent::CancelToken`] is retained so `Escape` can signal cooperative
/// cancellation.
pub(crate) fn spawn_agent(app: &mut App, agent: &mut Option<AgentSlot>, request: EffectRequest) {
    if app.runtime.run_state != RunState::Working {
        return;
    }
    if agent.is_some() {
        return;
    }

    app.runtime.stopping_timed_out = false;
    let prompt = active_provider_prompt(app);
    let cli = app.runtime.cli.clone();
    let workspace_root = crate::context::discover_workspace_root(&cli.cwd);
    let mut config = tools::AgentRunConfig::new(workspace_root, cli.model.clone(), cli.websearch)
        .with_authority(cli.authority)
        .with_search_url(cli.websearch_url.clone())
        .with_reasoning(cli.reasoning_effort, cli.reasoning_summary)
        .with_extra_read_roots(app.transcript.skills.iter().map(|skill| skill.root.clone()).collect())
        .with_model_reduction(app.effective_model_reduction())
        .with_process_registry(app.runtime.process_registry.clone());
    if let Some(store) = app.artifact_store() {
        config = config.with_artifact_store(store);
    }
    if let Some(acp_name) = acp::config::parse_model_id(&cli.model) {
        tracing::info!(
            cwd = %config.root.display(),
            model = %config.model,
            acp_agent = %acp_name,
            "spawning ACP agent run"
        );
        let (steering_tx, _steering_rx) = mpsc::channel();
        let mut handle = acp::runner::RunHandle::new(
            config.root,
            acp_name.to_string(),
            cli.acp_agents.get(acp_name).cloned(),
            prompt,
        );
        if let Ok(effective_mcp) = super::load_effective_mcp_for_workspace(&handle.root) {
            handle = handle
                .with_mcp_config(effective_mcp.config)
                .with_mcp_diagnostics(effective_mcp.diagnostics);
        }
        let cancel = handle.cancel.clone();
        let receiver = handle.spawn();
        *agent = Some(AgentSlot { request, receiver, cancel, steering: steering_tx });
        return;
    }
    let mcp_manager = super::load_mcp_manager_for_workspace(&config.root).ok();
    if let Some(manager) = mcp_manager.clone() {
        config = config.with_mcp_manager(manager);
    }
    tracing::info!(
        cwd = %config.root.display(),
        model = %config.model,
        requested_websearch = %cli.websearch.label(),
        search_backend = %config.search_mode.label(),
        "spawning agent run"
    );

    let tool_catalog = tools::runtime_tool_definitions(mcp_manager.as_deref());
    let ledger = app.refresh_context_ledger(Some(&prompt));
    let turn_id = format!("turn_{}", app.session.turn_count);
    config = config.with_request_context(turn_id, &ledger);
    let bundle = PromptBundle::new_with_skills(
        &config.root,
        &config.model,
        config.search_mode,
        &app.transcript.context_sources,
        &app.transcript.skills,
        &app.transcript.entries,
        &prompt,
    )
    .with_tool_catalog(tool_catalog)
    .with_context_ledger(ledger);

    if !app.compaction_in_flight() && preflight_requires_auto_compaction(app, &bundle) {
        start_auto_compaction(app, prompt);
        spawn_agent(app, agent, request);
        return;
    }

    if let Some(ref mut writer) = app.session.writer {
        let turn_id = format!("turn_{}", app.session.turn_count);
        let metadata = session::PromptMetadata::from_bundle(&bundle);
        let _ = writer.append_prompt_metadata(&turn_id, &metadata);
    }
    let messages = crate::prompt::lower_to_provider_messages(&bundle);
    let expects_write = agent::prompt_expects_workspace_write(&prompt);
    let (steering_tx, steering_rx) = mpsc::channel();
    let turn = harness::HarnessTurn::provider_with_steering(config, messages, expects_write, steering_rx).start();
    *agent = Some(AgentSlot { request, receiver: turn.events, cancel: turn.cancel, steering: steering_tx });
}

/// Headless adapter compatibility: execute the pending start effect directly.
pub(crate) fn maybe_spawn_agent(app: &mut App, agent: &mut Option<AgentSlot>) {
    if app.runtime.run_state != RunState::Working || agent.is_some() {
        return;
    }
    let request = app
        .runtime
        .active_effect_request
        .clone()
        .unwrap_or_else(|| EffectRequest { session_id: app.session.id.clone(), turn: app.session.turn_count });
    app.runtime.active_effect_request = Some(request.clone());
    spawn_agent(app, agent, request);
}

/// Return the prompt for the turn that is about to start.
///
/// Normal submitted prompts are present in the transcript, but internal turns
/// such as compaction must remain invisible there. `last_input` carries the
/// exact active provider prompt for both paths; the transcript fallback keeps
/// manually assembled application state usable in tests and adapters.
pub(crate) fn active_provider_prompt(app: &App) -> String {
    app.composer.last_input.clone().unwrap_or_else(|| {
        app.transcript
            .entries
            .iter()
            .rev()
            .find_map(|entry| match entry {
                app::Entry::User { text } => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default()
    })
}

/// Whether the upcoming provider request is oversized and needs auto-compaction.
///
/// Resolves model limits conservatively (static provider metadata or fallback;
/// live metadata loads inside the agent thread), estimates the full prompt
/// token cost from the lowered provider messages (which include the rendered
/// system prompt as the first message), and runs the pure
/// [`agent_context::preflight_auto_compaction`] decision.
///
/// Returns `false` when auto mode is disabled or the estimate fits the policy.
pub(crate) fn preflight_requires_auto_compaction(app: &App, bundle: &PromptBundle) -> bool {
    let policy = app.effective_compaction_policy();
    if !matches!(policy.mode, agent_context::CompactionMode::Auto) {
        return false;
    }
    let provider = acp::config::provider_label(&app.runtime.model);
    let (limits, _) = agent_context::ModelContextLimits::resolve(provider, &app.runtime.model, None, None);
    let messages = crate::prompt::lower_to_provider_messages(bundle);
    let bytes = messages.iter().map(|message| message.as_text().len()).sum::<usize>();
    let estimate = agent_context::estimate_tokens(bytes) as u64;
    matches!(
        agent_context::preflight_auto_compaction(policy, &limits, estimate),
        agent_context::AutoCompactionDecision::Compact
    )
}

pub(crate) fn flush_steering(app: &mut App, agent: &Option<AgentSlot>) {
    let Some(slot) = agent else {
        return;
    };
    let pending = app
        .composer
        .queue
        .items
        .iter()
        .filter(|item| item.target == app::QueueTarget::Steering && item.settlement == app::QueueSettlement::Pending)
        .map(|item| (item.id, item.text.clone()))
        .collect::<Vec<_>>();
    for (id, message) in pending {
        if slot.steering.send(message).is_ok() {
            let _ = app.composer.queue.settle(id, app::QueueSettlement::Sent);
            app::audit_queue_transition(app, id, "sent");
        }
    }
}

pub(crate) fn settle_agent(agent: &mut Option<AgentSlot>, request: &EffectRequest, stopping_timed_out: bool) {
    if agent.as_ref().is_some_and(|slot| &slot.request == request)
        && let Some(mut slot) = agent.take()
    {
        if stopping_timed_out {
            slot.receiver.detach();
        } else {
            slot.cancel.cancel();
            if let Err(error) = slot.receiver.wait() {
                tracing::error!(%error, "agent worker failed while settling");
            }
        }
    }
}
