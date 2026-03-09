mod app;
mod backend;
mod model;
mod storage;
mod view;

pub fn run() -> iced::Result {
    iced::application(app::boot, app::update, view::view)
        .title(app::title)
        .theme(app::theme)
        .subscription(app::subscription)
        .default_font(app::default_font())
        .window(app::window_settings())
        .run()
}
