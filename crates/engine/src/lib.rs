//! Raw Precision Touchpad input and pure gesture recognition.
mod config;
pub use config::*;
mod actions;
pub mod gestures;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
pub use platform::{windows_conflicts, Engine};

use serde::{Deserialize, Serialize};
pub const APP_NAME: &str = "Touch";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub trigger_prefix: &'static str,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineEvent {
    Touchpad { width_mm: f64, height_mm: f64 },
    NoTouchpad,
    Fired { trigger: String },
    Adjusted { trigger: String, steps: i32 },
    Error(String),
}

#[cfg(not(windows))]
pub fn windows_conflicts(_: &Config) -> Vec<Conflict> {
    vec![]
}
#[cfg(not(windows))]
pub struct Engine;
#[cfg(not(windows))]
impl Engine {
    pub fn start(
        _: Config,
        _: Option<std::sync::mpsc::Sender<EngineEvent>>,
    ) -> std::io::Result<Self> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Windows is required",
        ))
    }
    pub fn set_config(&self, _: Config) {}
    pub fn set_paused(&self, _: bool) {}
    pub fn run_action(&self, _: &Action) {}
    pub fn stop(self) {}
}
