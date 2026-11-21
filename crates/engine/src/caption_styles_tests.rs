use serde_json::json;

use crate::Session;

fn session() -> (Session, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("fc-cap-styles-{}-{:?}", std::process::id(), std::thread::current().id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = Session { prefs_path: Some(dir.join("preferences.json")), ..Session::default() };
    s.execute("file.openDemoProject", json!({})).unwrap();
    s.execute("captions.add", json!({"text": "x"})).unwrap();
    (s, dir)
}

fn style(s: &Session) -> filmcraft_project::CaptionStyle {
    s.active_sequence().unwrap().caption_tracks[0].style.clone()
}

fn names(s: &mut Session) -> Vec<(String, bool)> {
    let r = s.execute("captions.stylePresets", json!({})).unwrap();
    r["presets"].as_array().unwrap().iter().map(|p| (p["name"].as_str().unwrap().to_string(), p["builtin"].as_bool().unwrap())).collect()
}

#[test]
fn built_in_looks_apply_in_one_undo_step_and_own_looks_are_kept() {
    let (mut s, dir) = session();
    assert!(names(&mut s).contains(&("Reels yellow".to_string(), true)));
    let classic = style(&s);

    s.execute("captions.applyStylePreset", json!({"preset": "reels yellow"})).unwrap();
    let y = style(&s);
    assert!(!y.background && y.outline > 0.0 && y.color == [255, 212, 0, 255], "{y:?}");
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(style(&s), classic, "one undo step");

    s.execute("captions.setStyle", json!({"track": "C1", "size": 99.0, "outline": 5.0})).unwrap();
    s.execute("captions.saveStylePreset", json!({"name": "  Mine "})).unwrap();
    assert!(names(&mut s).contains(&("Mine".to_string(), false)));
    s.execute("captions.applyStylePreset", json!({"preset": "Classic"})).unwrap();
    assert_eq!(style(&s), classic);
    // a new session over the same data directory still has it
    let kept = std::fs::read(dir.join("caption-style-presets.json")).unwrap();
    assert!(String::from_utf8_lossy(&kept).contains("Mine"));
    s.execute("captions.applyStylePreset", json!({"preset": "mine"})).unwrap();
    assert_eq!((style(&s).size, style(&s).outline), (99.0, 5.0));

    s.execute("captions.deleteStylePreset", json!({"name": "MINE"})).unwrap();
    assert!(!names(&mut s).iter().any(|(n, _)| n == "Mine"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_requests_change_nothing() {
    let (mut s, dir) = session();
    let before = style(&s);
    for (cmd, p) in [
        ("captions.applyStylePreset", json!({"preset": "nope"})),
        ("captions.applyStylePreset", json!({})),
        ("captions.saveStylePreset", json!({"name": "classic"})),
        ("captions.saveStylePreset", json!({"name": "   "})),
        ("captions.saveStylePreset", json!({"name": "x".repeat(65)})),
        ("captions.deleteStylePreset", json!({"name": "Classic"})),
        ("captions.deleteStylePreset", json!({"name": "nothing saved"})),
    ] {
        assert!(s.execute(cmd, p.clone()).is_err(), "{cmd} {p}");
    }
    assert_eq!(style(&s), before);
    assert!(!dir.join("caption-style-presets.json").exists(), "nothing was written");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_damaged_file_is_an_empty_list_and_saving_replaces_it() {
    let (mut s, dir) = session();
    std::fs::write(dir.join("caption-style-presets.json"), b"{ not json").unwrap();
    assert_eq!(names(&mut s).iter().filter(|(_, b)| !b).count(), 0);
    s.execute("captions.saveStylePreset", json!({"name": "Fresh"})).unwrap();
    assert!(names(&mut s).contains(&("Fresh".to_string(), false)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_data_directory_saving_says_so() {
    let (mut s, dir) = session();
    s.prefs_path = None;
    let e = s.execute("captions.saveStylePreset", json!({"name": "Mine"})).unwrap_err().to_string();
    assert!(e.contains("data directory"), "{e}");
    s.execute("captions.applyStylePreset", json!({"preset": "Minimal"})).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

fn clips(s: &Session) -> usize {
    s.active_sequence().unwrap().video_tracks.iter().map(|t| t.items.len()).sum()
}

#[test]
fn a_template_turns_captions_into_graphics_in_one_undo_step() {
    let (mut s, dir) = session();
    s.execute("playhead.set", json!({"seconds": 4.0})).unwrap();
    s.execute("captions.add", json!({"text": "deux", "durationSeconds": 2.0})).unwrap();
    let caps = s.active_sequence().unwrap().caption_tracks[0].captions.clone();
    let before = clips(&s);
    s.state.caption_selection.clear();

    let r = s.execute("graphics.template.applyToCaptions", json!({"template": "builtin:subtitle-box"})).unwrap();
    let made: Vec<u64> = r["clips"].as_array().unwrap().iter().map(|c| c.as_u64().unwrap()).collect();
    assert_eq!(made.len(), 2, "{r}");
    assert_eq!(clips(&s), before + 2);
    assert_eq!(s.active_sequence().unwrap().caption_tracks[0].captions.len(), 2, "the captions are kept");
    for (clip, cap) in made.iter().zip(&caps) {
        let q = s.active_sequence().unwrap();
        let it = q.find_item(filmcraft_project::ClipId(*clip)).unwrap().1;
        assert_eq!((it.start, it.duration), (cap.start, cap.duration), "the clip spans its caption");
        let c = s.execute("graphics.template.controls", json!({"clip": clip})).unwrap();
        let text = c["controls"].as_array().unwrap().iter().find(|c| c["id"] == "text").unwrap()["value"].clone();
        assert_eq!(text, cap.text.as_str());
    }
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(clips(&s), before, "one undo step");

    s.execute("graphics.template.applyToCaptions", json!({"template": "builtin:subtitle-box", "replace": true, "captions": [caps[1].id.0]})).unwrap();
    assert_eq!((clips(&s), s.active_sequence().unwrap().caption_tracks[0].captions.len()), (before + 1, 1), "replace drops only that caption");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn applying_a_template_to_captions_refuses_what_it_cannot_do() {
    let (mut s, dir) = session();
    let before = clips(&s);
    for p in [
        json!({"template": "nope"}),
        json!({}),
        json!({"template": "builtin:subtitle-box", "captions": [999999]}),
        json!({"template": "builtin:lower-third-slab", "captions": [1]}),
    ] {
        let r = s.execute("graphics.template.applyToCaptions", p.clone());
        // the last one names no real caption, so it is also refused
        assert!(r.is_err(), "{p}");
    }
    assert_eq!(clips(&s), before, "a refused call changes nothing");
    let r = s.execute("graphics.template.list", json!({"category": "Captions"})).unwrap();
    assert_eq!(r.as_array().map_or(0, Vec::len), 2, "{r}");
    let _ = std::fs::remove_dir_all(&dir);
}
