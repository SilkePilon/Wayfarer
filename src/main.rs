mod application;
mod config;
mod controller;
mod engine;
mod models;
mod terrain;
mod widgets;
mod window;

fn main() {
    let app = application::WayfarerApp::new();
    std::process::exit(app.run());
}
