//! Compositor backends for window selection: one implementation per compositor.

pub mod niri;

/// A window, in output-local logical coordinates.
#[derive(Clone, Debug)]
pub struct Window {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub title: String,
    pub app_id: String,
    pub floating: bool,
}

/// Provides the on-screen windows of an output.
///
/// Implementations shell out to / talk to a specific compositor's IPC. Add a new
/// one here and register it in [`detect`].
pub trait WindowProvider {
    /// Short backend name, e.g. "niri".
    fn name(&self) -> &'static str;

    /// Windows currently visible on `output`, in output-local logical
    /// coordinates. `floating` windows should be last (topmost) for hit-testing.
    fn windows_on(&self, output: &str) -> Result<Vec<Window>, String>;
}

/// Pick a backend for the running compositor, if supported yet.
pub fn detect() -> Option<Box<dyn WindowProvider>> {
    if std::env::var_os("NIRI_SOCKET").is_some() {
        return Some(Box::new(niri::Niri));
    }
    // TODO: HYPRLAND_INSTANCE_SIGNATURE -> hyprland, SWAYSOCK -> sway, ...
    None
}
