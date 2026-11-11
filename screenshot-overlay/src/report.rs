//! The report a run leaves on stdout: what happened, and which button did it.
//!
//! This is the whole of the binary's outward contract beyond the image file
//! itself. Deciding *what to do* with a selection — clipboard, editor, OCR,
//! recording — belongs to the plugin, so this module deliberately does nothing
//! but describe the outcome. The plugin reads the button and the saved path out
//! of here and takes it from there.
//!
//! **What is *not* in here is what the selection was for.** There is no target
//! field: a caller that launched this binary chose the target, so echoing it back
//! would be telling it what it already knows — and the binary has no target
//! concept to name one with. The report describes the run, not the request.
//!
//! Written to stdout as one bare JSON object — the only thing on that stream —
//! with logs on stderr (`main.rs`). The image is always a file, so stdout is
//! free to carry it.

use std::path::Path;

/// The selected rectangle, in output-local **logical** pixels.
///
/// Logical rather than physical because that is the space a compositor tool
/// takes a geometry in (`wf-recorder --geometry`), and the space the plugin's
/// `noctalia.outputs()` reports each output's origin in — so a caller turns this
/// into a global geometry by adding the monitor's own `x`/`y`. The crop the
/// binary writes is physical, and is not what this describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Which mouse button finished the selection.
///
/// Reporting it is the binary's one piece of policy; what it *means* is the
/// plugin's decision. An enum rather than the `"left"` / `"right"` it used to be
/// carried as, because the plugin switches on this value and a typo would reach
/// it as an unknown button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
}

impl Button {
    fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// What became of the run, and everything the report has to say about it.
///
/// The fields live *in* the variants rather than beside them as `Option`s, so the
/// shape of the JSON is a fact about the type: a `Cancelled` report has nowhere to
/// put a region, and a `Delivered` one cannot omit its region. "A field that does
/// not apply is absent, never null" is the contract the plugin relies on, and this
/// is what makes it unbreakable rather than a rule every call site has to
/// remember. The one exception is `Delivered::output`, which is `Option` because a
/// caller can ask for the region *without* an image — see `cli::Args::region_only`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome<'a> {
    /// A selection was made, and written to a file unless the caller asked for the
    /// region only.
    Delivered {
        /// Which mouse button finished the selection.
        button: Button,
        /// Where the image was written, when one was. The caller chose this path,
        /// so this is how it finds the file again; absent means no image was
        /// asked for, not that writing one failed (that is `Failed`).
        output: Option<&'a Path>,
        /// The rectangle, in output-local **logical** pixels.
        region: Region,
        /// The connector name of the output the selection was made on.
        monitor: &'a str,
    },
    /// The user backed out — Esc, or the bar's close button. Not an error.
    Cancelled,
    /// Another overlay already held the single-instance lock, so this run did
    /// nothing. Not an error either: it is the answer to "may I start?".
    Busy,
    /// Something went wrong; the reason travels with it.
    Failed {
        /// Present when a selection *was* made and only the write failed, so the
        /// caller can still be told which button it was. Absent for a failure
        /// that happened before there was anything to report.
        button: Option<Button>,
        error: &'a str,
    },
}

impl Outcome<'_> {
    fn as_str(self) -> &'static str {
        match self {
            Self::Delivered { .. } => "delivered",
            Self::Cancelled => "cancelled",
            Self::Busy => "busy",
            Self::Failed { .. } => "failed",
        }
    }
}

/// Tell the caller what happened, as one JSON object on **stdout**.
///
/// stdout is the data channel and stderr is the log channel, so the two cannot
/// collide: the image is a file (named by the outcome's path, when there is one),
/// and that leaves stdout free to carry exactly this and nothing else. The plugin
/// reads it back through `runStream`'s line callback — the report is the binary's
/// only stdout line — and decodes it with `noctalia.json.decode`.
///
/// JSON rather than `key=value` because the interesting fields are file paths,
/// error messages and connector names, and all three can contain spaces and
/// quotes.
pub fn emit(outcome: Outcome) {
    println!("{}", to_json(outcome));
}

/// The report document, without the newline. Split out from [`emit`] so it can
/// be asserted on without capturing the process's stdout.
fn to_json(outcome: Outcome) -> String {
    let mut fields = serde_json::Map::new();
    fields.insert("result".into(), outcome.as_str().into());

    match outcome {
        Outcome::Delivered {
            button,
            output,
            region,
            monitor,
        } => {
            fields.insert("button".into(), button.as_str().into());
            if let Some(output) = output {
                fields.insert("output".into(), output.display().to_string().into());
            }
            fields.insert("monitor".into(), monitor.into());
            fields.insert(
                "region".into(),
                serde_json::json!({
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                }),
            );
        }
        // Nothing happened that needs describing — `result` is the whole of it.
        Outcome::Cancelled | Outcome::Busy => {}
        Outcome::Failed { button, error } => {
            if let Some(button) = button {
                fields.insert("button".into(), button.as_str().into());
            }
            fields.insert("error".into(), error.into());
        }
    }

    serde_json::Value::Object(fields).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The report is the whole of stdout, so the plugin can `noctalia.json.decode`
    /// it with no unwrapping. A delivered run carries the button, the path and the
    /// rectangle it selected.
    #[test]
    fn a_delivered_report_carries_the_path_and_region() {
        let delivered = to_json(Outcome::Delivered {
            button: Button::Right,
            output: Some(&PathBuf::from("/tmp/a b/c.png")),
            region: Region {
                x: 12,
                y: 34,
                w: 560,
                h: 315,
            },
            monitor: "DP-1",
        });
        let json: serde_json::Value = serde_json::from_str(&delivered).unwrap();
        // Five fields exactly. This is the plugin's contract, so pinning the count
        // is the strongest form of "nothing else leaked into it".
        assert_eq!(json.as_object().unwrap().len(), 5, "five fields, no more");
        assert_eq!(json["result"], "delivered");
        assert_eq!(json["button"], "right", "the button is the point");
        assert_eq!(json["output"], "/tmp/a b/c.png", "spaces survive");
        assert_eq!(json["monitor"], "DP-1");

        // The region is nested, and its four numbers must be numbers — a stringly
        // rectangle would compare equal to "12" and then fail arithmetic in Luau.
        let r = &json["region"];
        assert_eq!(r["x"], 12);
        assert_eq!(r["y"], 34);
        assert_eq!(r["w"], 560);
        assert_eq!(r["h"], 315);
        assert!(r["x"].is_i64(), "the region is not stringly: {r}");
    }

    /// A run asked for the region only has no path to carry, and says so by
    /// leaving the field out. The plugin's recording targets read the rectangle
    /// and never look at `output`; nothing was written, so nothing is claimed.
    #[test]
    fn a_delivered_report_without_an_image_omits_the_path() {
        let delivered = to_json(Outcome::Delivered {
            button: Button::Left,
            output: None,
            region: Region {
                x: 0,
                y: 0,
                w: 8,
                h: 6,
            },
            monitor: "eDP-1",
        });
        let json: serde_json::Value = serde_json::from_str(&delivered).unwrap();
        assert_eq!(json["result"], "delivered");
        assert!(
            json.get("output").is_none(),
            "no image was written, so there is no path to report: {json}"
        );
        assert_eq!(
            json.as_object().unwrap().len(),
            4,
            "result, button, monitor, region"
        );
    }

    /// A field that does not apply is absent, never null. A cancelled run selected
    /// nothing and a busy one did not fail, so there is nothing to put in those
    /// fields — and `result` alone is the whole of what either of them says.
    #[test]
    fn a_report_leaves_out_the_fields_that_do_not_apply() {
        let cancelled = to_json(Outcome::Cancelled);
        let json: serde_json::Value = serde_json::from_str(&cancelled).unwrap();
        assert_eq!(json["result"], "cancelled");
        assert_eq!(json.as_object().unwrap().len(), 1, "nothing else");
        assert!(
            json.get("target").is_none(),
            "the report does not echo what the selection was for"
        );

        let busy = to_json(Outcome::Busy);
        let json: serde_json::Value = serde_json::from_str(&busy).unwrap();
        assert_eq!(json["result"], "busy");
        assert_eq!(json.as_object().unwrap().len(), 1, "nothing else");

        // A failure before there was a selection: a reason, but no button.
        let failed = to_json(Outcome::Failed {
            button: None,
            error: "the lock file is not reachable",
        });
        let json: serde_json::Value = serde_json::from_str(&failed).unwrap();
        assert!(json.get("button").is_none(), "nothing was selected");
    }

    /// The failure reason travels too, quotes included: it is a message from this
    /// program that the plugin shows verbatim, so escaping has to survive the trip.
    /// A write that failed still knows which button asked for it.
    #[test]
    fn a_failed_report_carries_its_reason() {
        let failed = to_json(Outcome::Failed {
            button: Some(Button::Left),
            error: "saving /nope \"x\" failed",
        });
        let json: serde_json::Value = serde_json::from_str(&failed).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 3, "and nothing else");
        assert_eq!(json["result"], "failed");
        assert_eq!(json["button"], "left", "the selection that failed to save");
        assert_eq!(json["error"], "saving /nope \"x\" failed");
    }
}
