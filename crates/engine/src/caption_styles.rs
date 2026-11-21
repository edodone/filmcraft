//! One-click caption looks: a few built-in ones and the user's own, kept in the data directory
//! (`caption-style-presets.json`). A look is a whole [`CaptionStyle`]; applying one replaces the
//! style of a caption track.

use std::path::PathBuf;

use filmcraft_project::{CaptionAnchor, CaptionStyle};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::commands::{CommandSpec, NamedKeys, always, bad, str_p};
use crate::{EngineError, Result, Session};

type Run = fn(&mut Session, &Value) -> Result<Value>;
type Enabled = fn(&Session) -> std::result::Result<(), String>;

const FILE: &str = "caption-style-presets.json";
const MAX_SAVED: usize = 200;
const MAX_FILE_BYTES: u64 = 1 << 20;

#[derive(Clone, Serialize, Deserialize)]
struct Saved {
    name: String,
    style: CaptionStyle,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct SavedFile {
    format: String,
    version: u32,
    presets: Vec<Saved>,
}

fn path(s: &Session) -> Option<PathBuf> {
    s.prefs_path.as_ref()?.parent().map(|d| d.join(FILE))
}

/// The user's looks; a missing, oversized or damaged file is an empty list.
fn load(s: &Session) -> Vec<Saved> {
    let Some(p) = path(s) else { return Vec::new() };
    if std::fs::metadata(&p).map_or(true, |m| m.len() > MAX_FILE_BYTES) {
        return Vec::new();
    }
    std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<SavedFile>(&b).ok()).map(|f| f.presets).unwrap_or_default()
}

fn store(s: &Session, presets: &[Saved]) -> Result<()> {
    let p = path(s).ok_or_else(|| EngineError::Other("no data directory to keep caption styles in".into()))?;
    let file = SavedFile { format: "filmcraft.caption-styles".into(), version: 1, presets: presets.to_vec() };
    let bytes = serde_json::to_vec_pretty(&file).map_err(|e| EngineError::Other(e.to_string()))?;
    filmcraft_format::atomic_write(&p, &bytes).map_err(|e| EngineError::Other(format!("{}: {e}", p.display())))
}

fn rgba(rgb: u32, alpha: u8) -> [u8; 4] {
    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, alpha]
}

fn look(font_style: &str, size: f32, color: [u8; 4], background: Option<[u8; 4]>, outline: f32, anchor: CaptionAnchor, margin: f32) -> CaptionStyle {
    CaptionStyle {
        font_style: font_style.into(),
        size,
        color,
        background: background.is_some(),
        background_color: background.unwrap_or([0, 0, 0, 191]),
        outline,
        anchor,
        margin,
        ..CaptionStyle::default()
    }
}

/// The built-in looks (original designs; sizes are for a 1080-line frame).
pub fn builtin() -> Vec<(&'static str, CaptionStyle)> {
    let white = rgba(0xffffff, 255);
    vec![
        ("Classic", CaptionStyle::default()),
        ("Reels white", look("Bold", 76.0, white, None, 7.0, CaptionAnchor::Bottom, 0.2)),
        ("Reels yellow", look("Bold", 76.0, rgba(0xffd400, 255), None, 7.0, CaptionAnchor::Bottom, 0.2)),
        ("Black box", look("Bold", 60.0, white, Some(rgba(0x000000, 230)), 0.0, CaptionAnchor::Bottom, 0.1)),
        ("White box", look("Bold", 60.0, rgba(0x111111, 255), Some(rgba(0xffffff, 235)), 0.0, CaptionAnchor::Bottom, 0.1)),
        ("Minimal", look("Medium", 44.0, white, None, 3.0, CaptionAnchor::Bottom, 0.06)),
        ("Top banner", look("Bold", 64.0, white, Some(rgba(0x1d4ed8, 220)), 0.0, CaptionAnchor::Top, 0.08)),
    ]
}

fn is_builtin(name: &str) -> bool {
    builtin().iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
}

fn find(s: &Session, name: &str) -> Option<CaptionStyle> {
    builtin()
        .into_iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, st)| st)
        .or_else(|| load(s).into_iter().find(|u| u.name.eq_ignore_ascii_case(name)).map(|u| u.style))
}

fn list(s: &mut Session, _: &Value) -> Result<Value> {
    let mut out: Vec<Value> = builtin().into_iter().map(|(n, st)| json!({"name": n, "builtin": true, "style": st})).collect();
    out.extend(load(s).into_iter().map(|u| json!({"name": u.name, "builtin": false, "style": u.style})));
    Ok(json!({"presets": out}))
}

fn track(s: &Session, p: &Value, cmd: &str) -> Result<filmcraft_project::TrackId> {
    crate::captions::track_param(s, p).ok_or_else(|| bad(cmd, "no such caption track (pass `track`: an id from captions.list, or \"C1\"…)"))
}

fn apply(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "captions.applyStylePreset";
    let name = str_p(p, "preset").ok_or_else(|| bad(cmd, "`preset` is required (see captions.stylePresets)"))?;
    let style = find(s, name).ok_or_else(|| bad(cmd, format!("no caption style named `{name}`")))?;
    let tid = track(s, p, cmd)?;
    s.edit("Caption Style Preset", |pr, st| {
        let seq = st.active_sequence.ok_or(EngineError::NoSequence)?;
        pr.sequence_mut(seq).and_then(|q| q.caption_track_mut(tid)).ok_or(EngineError::NoSequence)?.style = style;
        Ok(())
    })?;
    Ok(Value::Null)
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "captions.saveStylePreset";
    let name = str_p(p, "name")
        .map(str::trim)
        .filter(|n| !n.is_empty() && n.chars().count() <= 64)
        .ok_or_else(|| bad(cmd, "`name` is required (1 to 64 characters)"))?;
    if is_builtin(name) {
        return Err(bad(cmd, format!("`{name}` is a built-in style; pick another name")));
    }
    let tid = track(s, p, cmd)?;
    let style = s.active_sequence().and_then(|q| q.caption_track(tid)).map(|t| t.style.clone()).ok_or(EngineError::NoSequence)?;
    let mut all = load(s);
    all.retain(|u| !u.name.eq_ignore_ascii_case(name));
    if all.len() >= MAX_SAVED {
        return Err(bad(cmd, format!("at most {MAX_SAVED} saved styles; delete some first")));
    }
    all.push(Saved { name: name.to_string(), style });
    store(s, &all)?;
    Ok(json!({"name": name}))
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "captions.deleteStylePreset";
    let name = str_p(p, "name").ok_or_else(|| bad(cmd, "`name` is required"))?;
    let mut all = load(s);
    let n = all.len();
    all.retain(|u| !u.name.eq_ignore_ascii_case(name));
    if all.len() == n {
        return Err(bad(cmd, format!("no saved caption style named `{name}` (built-in styles cannot be deleted)")));
    }
    store(s, &all)?;
    Ok(Value::Null)
}

fn spec(id: &'static str, label: &'static str, params: &'static str, enabled: Enabled, run: Run, journal: bool) -> CommandSpec {
    CommandSpec { id, label, menu: &[], shortcut: None, params, enabled, run, journal, named: NamedKeys::NONE }
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec("captions.stylePresets", "List Caption Styles", "{}", always, list, false),
        spec("captions.applyStylePreset", "Apply Caption Style", r#"{"track":id|"C1"?,"preset":str}"#, crate::captions::has_caption_track, apply, true),
        spec("captions.saveStylePreset", "Save Caption Style", r#"{"track":id|"C1"?,"name":str}"#, crate::captions::has_caption_track, save, false),
        spec("captions.deleteStylePreset", "Delete Saved Caption Style", r#"{"name":str}"#, always, delete, false),
    ]
}

/// Every look's name, the built-in ones first, with whether it is built in (for menus).
pub fn names(s: &Session) -> Vec<(String, bool)> {
    let mut v: Vec<(String, bool)> = builtin().into_iter().map(|(n, _)| (n.to_string(), true)).collect();
    v.extend(load(s).into_iter().map(|u| (u.name, false)));
    v
}
