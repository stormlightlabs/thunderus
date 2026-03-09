use super::backend::{BackendBridge, spawn_backend};
use super::model::{AppModel, BackendEvent, BootstrapState, Effect, Message, ModelMessage};
use super::model::{collect_workspace_files, color_hex, update_model};
use super::storage::{PersistedUiState, StateStore};
use iced::font::Family;
use iced::task::Task;
use iced::theme::Palette;
use iced::{Font, Subscription, Theme, window};
use std::time::Duration;

const POLL_INTERVAL_MS: u64 = 16;
const WINDOW_WIDTH: f32 = 1280.0;
const WINDOW_HEIGHT: f32 = 840.0;

const JETBRAINS_MONO: Font = Font { family: Family::Name("JetBrains Mono"), ..Font::MONOSPACE };

#[derive(Debug)]
pub struct DesktopApp {
    pub model: AppModel,
    backend: Option<BackendBridge>,
    store: StateStore,
}

pub fn boot() -> DesktopApp {
    let store = StateStore::new_default();
    let persisted = store.load();

    let mut warning = None;
    let workspace_root = persisted.workspace_root.and_then(|path| {
        if path.is_dir() {
            Some(path)
        } else {
            warning = Some(format!("Saved workspace does not exist: {}", path.display()));
            None
        }
    });

    let bootstrap = BootstrapState {
        workspace_root: workspace_root.clone(),
        recent_workspaces: persisted.recent_workspaces,
        composer_text: persisted.composer_text,
        last_model: persisted.last_model,
        warning,
    };

    DesktopApp { model: AppModel::new(bootstrap), backend: workspace_root.map(spawn_backend), store }
}

impl DesktopApp {
    fn drain_backend(&mut self) -> Task<Message> {
        let _ = update_model(&mut self.model, ModelMessage::Tick);

        let Some(backend) = self.backend.as_mut() else {
            return Task::none();
        };

        let mut events = Vec::new();
        while let Ok(event) = backend.event_rx.try_recv() {
            events.push(event);
        }

        let mut all_effects = Vec::new();
        for event in events {
            let effects = update_model(&mut self.model, ModelMessage::BackendEvent(event));
            all_effects.extend(effects);
        }

        self.run_effects(all_effects)
    }

    fn run_effects(&mut self, effects: Vec<Effect>) -> Task<Message> {
        let mut tasks = Vec::new();

        for effect in effects {
            match effect {
                Effect::DispatchPrompt(prompt) => {
                    tracing::info!("Dispatching prompt from GUI shell chars={}", prompt.chars().count());
                    if let Some(backend) = self.backend.as_ref()
                        && let Err(error) = backend.request_tx.send(prompt)
                    {
                        tracing::error!("Backend request send failed: {error}");
                        let _ = update_model(
                            &mut self.model,
                            ModelMessage::BackendEvent(BackendEvent::Error(format!(
                                "Backend request failed: {}",
                                error
                            ))),
                        );
                    }
                }
                Effect::OpenWorkspacePicker => tasks
                    .push(Task::perform(async { rfd::FileDialog::new().pick_folder() }, |path| {
                        Message::Model(ModelMessage::WorkspacePicked(path))
                    })),
                Effect::DeactivateWorkspace => self.backend = None,
                Effect::ActivateWorkspace(workspace_root) => {
                    tracing::info!("Activating workspace {}", workspace_root.display());
                    self.backend = Some(spawn_backend(workspace_root));
                }
                Effect::LoadWorkspaceFiles(workspace_root) => tasks.push(Task::perform(
                    async move { collect_workspace_files(&workspace_root) },
                    |files| Message::Model(ModelMessage::WorkspaceFilesLoaded(files)),
                )),
                Effect::PersistState => {
                    if let Err(error) = self.store.save(&self.persisted_state()) {
                        tracing::error!("Failed to persist desktop state: {error}");
                        self.model.error_text = Some(format!("Failed to persist desktop state: {error}"));
                    }
                }
            }
        }

        if tasks.is_empty() { Task::none() } else { Task::batch(tasks) }
    }

    fn persisted_state(&self) -> PersistedUiState {
        PersistedUiState {
            workspace_root: self.model.workspace_root.clone(),
            recent_workspaces: self.model.recent_workspaces.clone(),
            composer_text: self.model.composer_text.clone(),
            last_model: self.model.last_model.clone(),
        }
    }
}

pub fn update(app: &mut DesktopApp, message: Message) -> Task<Message> {
    match message {
        Message::Model(model_message) => {
            let effects = update_model(&mut app.model, model_message);
            app.run_effects(effects)
        }
        Message::PollBackend(_instant) => app.drain_backend(),
    }
}

pub fn title(_: &DesktopApp) -> String {
    "Thunderus - Desktop".to_string()
}

pub fn theme(_: &DesktopApp) -> Theme {
    Theme::custom(
        "Oxocarbon Dark".to_string(),
        Palette {
            background: color_hex("#161616"),
            text: color_hex("#f4f4f4"),
            primary: color_hex("#33b1ff"),
            success: color_hex("#42be65"),
            warning: color_hex("#f1c21b"),
            danger: color_hex("#fa4d56"),
        },
    )
}

pub fn subscription(app: &DesktopApp) -> Subscription<Message> {
    match &app.backend {
        Some(_) => iced::time::every(Duration::from_millis(POLL_INTERVAL_MS)).map(Message::PollBackend),
        None => Subscription::none(),
    }
}

pub fn window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(WINDOW_WIDTH, WINDOW_HEIGHT),
        position: window::Position::Centered,
        ..window::Settings::default()
    }
}

pub fn default_font() -> Font {
    JETBRAINS_MONO
}
