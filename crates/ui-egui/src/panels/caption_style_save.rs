//! Save the caption track's style under a name, to apply it again in one click
//! (`captions.saveStylePreset`).
use crate::FilmcraftApp;
use serde_json::json;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Draft {
    project: Arc<()>,
    track: u64,
    name: String,
    error: String,
    focus: bool,
}

/// Open the window for caption track `track` of the active sequence.
pub fn open(app: &mut FilmcraftApp, track: u64) {
    app.ui.caption_style_save = Some(Draft { project: app.session.project_instance.clone(), track, name: String::new(), error: String::new(), focus: true });
}

pub fn show(app: &mut FilmcraftApp, ctx: &egui::Context) {
    let Some(mut draft) = app.ui.caption_style_save.take() else { return };
    // the project was replaced while the window was open: the track id means nothing now
    if !Arc::ptr_eq(&draft.project, &app.session.project_instance) {
        return;
    }
    let (mut save, mut cancel) = (false, false);
    let modal = egui::Modal::new(egui::Id::unique("caption-style-save")).show(ctx, |ui| {
        ui.set_width((ctx.content_rect().width() - 40.0).clamp(240.0, 360.0));
        ui.heading(tl!("Save Caption Style"));
        ui.label(tl!("Name for the current style of this caption track:"));
        let response = ui.add(egui::TextEdit::singleline(&mut draft.name).desired_width(f32::INFINITY));
        app.auto.add_response("captionStyleSave.name", &response, tl!("Style name"));
        if std::mem::take(&mut draft.focus) {
            response.request_focus();
        }
        // Enter in the field saves
        let entered = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if !draft.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &draft.error);
        }
        ui.horizontal(|ui| {
            let ok = ui.add_enabled(!draft.name.trim().is_empty(), egui::Button::new(tl!("Save")));
            app.auto.add_response("captionStyleSave.save", &ok, tl!("Save"));
            save = ok.clicked() || (entered && !draft.name.trim().is_empty());
            let no = ui.button(tl!("Cancel"));
            app.auto.add_response("captionStyleSave.cancel", &no, tl!("Cancel"));
            cancel = no.clicked();
        });
    });
    if cancel || modal.should_close() {
        return;
    }
    if save {
        match app.session.execute("captions.saveStylePreset", json!({"track": draft.track, "name": draft.name})) {
            Ok(_) => return,
            Err(e) => draft.error = e.to_string(),
        }
    }
    app.ui.caption_style_save = Some(draft);
}
