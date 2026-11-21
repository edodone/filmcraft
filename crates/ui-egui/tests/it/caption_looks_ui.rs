//! The Captions tab menus: one click applies a look to the caption track, and a graphic template
//! turns the captions into clips.

use std::sync::mpsc::{Sender, channel};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::ControlRequest;
use serde_json::{Value, json};

struct Driver {
    harness: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
}

impl Driver {
    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            let ctx = self.harness.ctx.clone();
            let mut raw = std::mem::take(self.harness.input_mut());
            eframe::App::raw_input_hook(self.harness.state_mut(), &ctx, &mut raw);
            *self.harness.input_mut() = raw;
            self.harness.step();
        }
    }
    fn ok(&mut self, method: &str, params: Value) -> Value {
        let (req, reply) = ControlRequest::new(method, params.clone());
        self.tx.send(req).expect("control channel");
        for _ in 0..600 {
            self.frames(1);
            if let Ok(r) = reply.try_recv() {
                assert_eq!(r["ok"], true, "{method} {params}: {r}");
                return r["result"].clone();
            }
        }
        panic!("no reply to {method} {params}");
    }
    fn exec(&mut self, command: &str, params: Value) -> Value {
        self.ok("engine.execute", json!({"command": command, "params": params}))
    }
    fn ids(&mut self, prefix: &str) -> Vec<String> {
        self.frames(2);
        let v = self.ok("ui.elements", json!({"prefix": prefix}));
        v.as_array().expect("elements").iter().filter_map(|e| e["id"].as_str().map(str::to_string)).collect()
    }
    fn captions(&mut self) -> Vec<Value> {
        let r = self.exec("captions.list", json!({}));
        r["tracks"][0]["captions"].as_array().expect("captions").clone()
    }
}

fn caption_session() -> (Driver, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("filmcraft-caption-looks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let srt = dir.join("two.srt");
    std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,500\nBonjour\n\n2\n00:00:03,000 --> 00:00:04,000\nça va\n").expect("write srt");
    let mut session = Session::default();
    session.execute("file.openDemoProject", json!({})).expect("demo project");
    let (tx, rx) = channel();
    let app = FilmcraftApp::new(session).with_control(rx);
    let harness = Harness::builder().with_size(egui::vec2(1600.0, 980.0)).with_max_steps(10_000).build_eframe(move |_cc| app);
    let mut d = Driver { harness, tx };
    d.frames(4);
    d.exec("captions.import", json!({"path": srt.to_string_lossy()}));
    d.ok("ui.set", json!({"workspace": "Captions and Graphics"}));
    (d, dir)
}

#[test]
fn the_styles_menu_applies_a_look_in_one_click() {
    let (mut d, dir) = caption_session();
    let before = d.harness.state().session.active_sequence().unwrap().caption_tracks[0].style.clone();
    d.ok("ui.click", json!({"id": "text.captions.looks"}));
    d.frames(3);
    assert!(d.ids("text.captions.look.").iter().any(|s| s == "text.captions.look.Reels yellow"), "the menu lists the looks");
    d.ok("ui.click", json!({"id": "text.captions.look.Reels yellow"}));
    d.frames(3);
    let after = d.harness.state().session.active_sequence().unwrap().caption_tracks[0].style.clone();
    assert_ne!(after, before, "the look changed the style");
    assert!(!after.background && after.outline > 0.0, "{after:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_template_menu_makes_a_clip_for_each_caption() {
    let (mut d, dir) = caption_session();
    let clips = |d: &Driver| d.harness.state().session.active_sequence().unwrap().video_tracks.iter().map(|t| t.items.len()).sum::<usize>();
    let before = clips(&d);
    d.ok("ui.click", json!({"id": "text.captions.templates"}));
    d.frames(3);
    let listed = d.ids("text.captions.template.");
    assert!(listed.iter().any(|s| s == "text.captions.template.builtin:subtitle-box"), "{listed:?}");
    d.ok("ui.click", json!({"id": "text.captions.template.builtin:subtitle-box"}));
    d.frames(3);
    assert_eq!(clips(&d), before + 2, "one clip per caption");
    assert_eq!(d.captions().len(), 2, "the captions are kept");
    let _ = std::fs::remove_dir_all(&dir);
}
