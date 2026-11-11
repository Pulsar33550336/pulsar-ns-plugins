//! niri backend, via `niri msg -j ...`.
//!
//! A position is relative to the view of the workspace that window belongs to —
//! so a window on another output, or scrolled off the side of this one, has
//! coordinates that mean nothing on this surface. The output's *active
//! workspace* is what selects the windows actually visible here, and those
//! positions are logical pixels.
//!
//! niri reports that position twice, and this backend prefers the newer one:
//!
//!   * `layout.tile_pos_in_workspace_view` in `niri msg -j windows` (newer niri;
//!     one call, no id matching),
//!   * `niri msg -j window-geometries`, a flat list over every window (older
//!     niri, kept as the fallback so a slightly older compositor still works).
//!
//! Either way the actual window rect is the tile position plus
//! `window_offset_in_tile`, sized `window_size`.

use serde::Deserialize;

use super::{Window, WindowProvider};

pub struct Niri;

#[derive(Deserialize)]
struct NiriWindow {
    id: u64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    app_id: Option<String>,
    workspace_id: u64,
    is_floating: bool,
    layout: NiriLayout,
}

#[derive(Deserialize)]
struct NiriLayout {
    window_size: [f64; 2],
    window_offset_in_tile: [f64; 2],
    /// Newer niri only; absent on older releases, which sends us to
    /// `window-geometries` below.
    #[serde(default)]
    tile_pos_in_workspace_view: Option<[f64; 2]>,
}

#[derive(Deserialize)]
struct NiriGeom {
    id: u64,
    x: i32,
    y: i32,
}

#[derive(Deserialize)]
struct NiriWorkspace {
    id: u64,
    #[serde(default)]
    output: Option<String>,
    is_active: bool,
}

fn niri_json<T: for<'de> Deserialize<'de>>(args: &[&str]) -> Result<Vec<T>, String> {
    let out = std::process::Command::new("niri")
        .args(args)
        .output()
        .map_err(|e| format!("spawn niri: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "niri {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("parse niri {args:?}: {e}"))
}

impl WindowProvider for Niri {
    fn name(&self) -> &'static str {
        "niri"
    }

    fn windows_on(&self, output: &str) -> Result<Vec<Window>, String> {
        let workspaces: Vec<NiriWorkspace> = niri_json(&["msg", "-j", "workspaces"])?;
        // This output's active workspace, or nothing. Falling back to whichever
        // workspace is active elsewhere would not be a degraded highlight but a
        // wrong one: those windows carry the *other* output's view coordinates.
        let Some(active_ws) = workspaces
            .iter()
            .find(|w| w.is_active && w.output.as_deref() == Some(output))
            .map(|w| w.id)
        else {
            return Ok(Vec::new());
        };

        let wins: Vec<NiriWindow> = niri_json(&["msg", "-j", "windows"])?;
        // The legacy list is fetched at most once, and only if a window on this
        // workspace actually needs it: current niri (2026-11) dropped
        // `window-geometries` entirely, so asking unconditionally would spawn a
        // process that fails on every capture.
        let mut geoms: Option<Vec<NiriGeom>> = None;

        let mut out = Vec::new();
        for win in wins.iter().filter(|w| w.workspace_id == active_ws) {
            let l = &win.layout;
            let tile = match l.tile_pos_in_workspace_view {
                Some(p) => (p[0].round() as i32, p[1].round() as i32),
                None => {
                    let geoms = geoms.get_or_insert_with(|| {
                        niri_json(&["msg", "-j", "window-geometries"]).unwrap_or_else(|err| {
                            // Expected on current niri: the request no longer
                            // exists, so windows without a view position simply
                            // stay out of the highlight.
                            log::debug!("[windows] niri: {err}");
                            Vec::new()
                        })
                    });
                    let Some(g) = geoms.iter().find(|g| g.id == win.id) else {
                        // No geometry from either source: a window we cannot place
                        // is skipped rather than guessed at.
                        continue;
                    };
                    (g.x, g.y)
                }
            };
            let x = tile.0 + l.window_offset_in_tile[0].round() as i32;
            let y = tile.1 + l.window_offset_in_tile[1].round() as i32;
            let w = l.window_size[0].round() as i32;
            let h = l.window_size[1].round() as i32;
            if w <= 0 || h <= 0 {
                continue;
            }
            out.push(Window {
                x,
                y,
                w,
                h,
                title: win.title.clone().unwrap_or_default(),
                app_id: win.app_id.clone().unwrap_or_default(),
                floating: win.is_floating,
            });
        }
        // Floating windows are on top: keep them last for reverse hit-testing.
        out.sort_by_key(|w| w.floating);
        Ok(out)
    }
}
