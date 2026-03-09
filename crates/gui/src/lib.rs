mod app;
mod backend;
mod logging;
mod model;
mod storage;
mod view;

const POLL_INTERVAL_MS: u64 = 16;
const WINDOW_WIDTH: f32 = 1280.0;
const WINDOW_HEIGHT: f32 = 840.0;

const JETBRAINS_MONO: iced::Font =
    iced::Font { family: iced::font::Family::Name("JetBrains Mono"), ..iced::Font::MONOSPACE };

fn default_font() -> iced::Font {
    JETBRAINS_MONO
}

pub fn run() -> iced::Result {
    let initial_workspace = storage::StateStore::new_default()
        .load()
        .workspace_root
        .filter(|path| path.is_dir())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let verbose = std::env::var("THNDRS_GUI_VERBOSE")
        .ok()
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "on" | "ON"))
        .unwrap_or(false);

    let _tracing_runtime = match logging::init_gui_tracing(verbose, &initial_workspace) {
        Ok(runtime) => {
            let active_log_file = logging::latest_workspace_log_file(&runtime.log_dir)
                .ok()
                .flatten()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<pending>".to_string());
            tracing::info!(
                "GUI tracing initialized log_dir={} active_file={}",
                runtime.log_dir.display(),
                active_log_file
            );
            Some(runtime)
        }
        Err(error) => {
            eprintln!("GUI tracing disabled: {error}");
            None
        }
    };

    iced::application(app::boot, app::update, view::view)
        .title(app::title)
        .theme(app::theme)
        .subscription(app::subscription)
        .default_font(default_font())
        .window(app::window_settings())
        .run()
}
