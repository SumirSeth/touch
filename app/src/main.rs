#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! `touch.exe` runs the headless agent (gesture engine + tray). `touch.exe --settings` opens the settings
//! window in its own short-lived process. A second launch opens settings; `--quit` stops the agent.
mod agent;
mod ipc;
mod platform;
mod settings;
mod ui;
pub use settings::AppEvent;
pub const PRODUCT_NAME: &str = "Touch";

fn main() {
    let has = |flag: &str| std::env::args().any(|a| a == flag);
    if has("--settings") {
        settings::run();
        return;
    }
    match ipc::instance(ipc::AGENT) {
        Some(instance) if !has("--quit") => agent::run(instance, !has("--background")),
        Some(_) => {}
        None => ipc::signal(if has("--quit") {
            ipc::AGENT_QUIT
        } else {
            ipc::AGENT_OPEN
        }),
    }
}
