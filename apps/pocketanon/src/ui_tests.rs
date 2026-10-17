//! Headless UI tests with egui_kittest (no GPU needed).

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::PocketAnon;
use crate::services::Services;
use crate::services::stubs::{FakeModelManager, PlainTextLoader, demo_detector};
use crate::settings::SettingsStore;

fn app() -> PocketAnon {
    let services = Services {
        detectors: vec![Box::new(demo_detector())],
        documents: Box::new(PlainTextLoader),
        models: Box::new(FakeModelManager),
    };
    PocketAnon::new(None, services, SettingsStore::in_memory())
}

fn wait(h: &mut Harness<'_, PocketAnon>) {
    for _ in 0..200 {
        h.run_steps(1);
        if !h.state().jobs.busy() {
            h.run_steps(2);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("background job did not finish");
}

#[test]
fn type_detect_and_apply() {
    let mut h = Harness::builder().with_size(egui::vec2(1360.0, 840.0)).build_ui_state(|ui, app: &mut PocketAnon| app.show(ui), app());
    wait(&mut h); // model load
    h.get_by_role(egui::accesskit::Role::MultilineTextInput).focus();
    h.run_steps(1);
    h.get_by_role(egui::accesskit::Role::MultilineTextInput).type_text("Please call Max Mustermann at max@example.com today.");
    h.run_steps(2);
    assert!(h.state().text.input.contains("Mustermann"));
    h.get_by_role_and_label(egui::accesskit::Role::Button, "🔍  Detect").click();
    h.run_steps(1);
    wait(&mut h);
    let reviews = h.state().text.reviews.as_ref().expect("review present");
    let ents = reviews[0].review.entities();
    assert_eq!(ents.len(), 2, "{ents:?}");
    // The candidate row shows up in the review list.
    assert!(h.query_all_by_label_contains("Max Mustermann").next().is_some());
    h.get_all_by_label("Apply").next().expect("Apply button").click();
    h.run_steps(2);
    assert_eq!(h.state().text.output, "Please call [PERSON_1] at [EMAIL_1] today.");
    assert_eq!(h.state().text.summary.as_ref().unwrap().total(), 2);
}

/// Renders the main screens to PNGs when `POCKETANON_SCREENSHOTS=<dir>` is set (GPU via wgpu).
/// `POCKETANON_SCREENSHOTS=/tmp/shots cargo test -p pocketanon screenshots`
#[test]
fn screenshots() {
    let Ok(dir) = std::env::var("POCKETANON_SCREENSHOTS") else { return };
    let dir = std::path::PathBuf::from(dir);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1360.0, 840.0))
        .with_pixels_per_point(2.0)
        .wgpu()
        .build_ui_state(|ui, app: &mut PocketAnon| app.show(ui), app());
    wait(&mut h);
    let shot = |h: &mut Harness<'_, PocketAnon>, name: &str| {
        h.run_steps(3);
        h.render().expect("render").save(dir.join(name)).expect("save png");
    };
    shot(&mut h, "01-text-empty.png");
    h.state_mut().text.input = "Sehr geehrte Frau Erika Musterfrau,\n\nvielen Dank für Ihre Nachricht an erika.musterfrau@example.com. \
Herr Max Mustermann aus Berlin meldet sich nächste Woche.\n\nBitte überweisen Sie den Betrag auf DE89 3704 0044 0532 0130 00.\n\nMit freundlichen Grüßen\nJonas Beispiel"
        .into();
    h.state_mut().detect_text();
    h.run_steps(1);
    wait(&mut h);
    shot(&mut h, "02-text-review.png");
    h.state_mut().apply_text();
    shot(&mut h, "03-text-result.png");
    h.state_mut().settings_open = true;
    shot(&mut h, "04-settings.png");
    h.state_mut().settings_open = false;
    h.state_mut().palette.toggle();
    shot(&mut h, "05-palette.png");
    h.state_mut().palette.toggle();
    h.state_mut().workspace = crate::app::Workspace::File;
    shot(&mut h, "06-file-empty.png");
    let input = std::env::temp_dir().join("pocketanon-shot-brief.txt");
    std::fs::write(&input, "Betreff: Vertrag mit Max Mustermann\n\nhallo Erika Musterfrau,\n\nbitte melden Sie sich unter erika@example.com.\n\nviele Grüße\nJonas Beispiel").unwrap();
    h.state_mut().open_path(input.clone());
    wait(&mut h);
    h.state_mut().detect_file();
    h.run_steps(1);
    wait(&mut h);
    shot(&mut h, "07-file-review.png");
    std::fs::remove_file(&input).ok();
}

/// Also exercises the File workspace with a TXT document through the stub loader.
#[test]
fn file_workspace_detect_and_save() {
    let dir = std::env::temp_dir().join(format!("pocketanon-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("letter.txt");
    std::fs::write(&input, "hi Max Mustermann,\n\nwrite to max@example.com.\n\nregards, Max Mustermann").unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1360.0, 840.0)).build_ui_state(|ui, app: &mut PocketAnon| app.show(ui), app());
    wait(&mut h);
    h.state_mut().open_path(input.clone());
    wait(&mut h);
    assert_eq!(h.state().file.segments.len(), 3);
    h.state_mut().detect_file();
    wait(&mut h);
    let reviews = h.state().file.reviews.as_ref().unwrap();
    let (plan, summary) = crate::review_ui::plan_all(reviews, pocketanon_core::Strategy::IndexedRedaction);
    assert_eq!(summary.total(), 3);
    // Same person in body paragraph 1 and 3 gets the same index across segments.
    assert_eq!(plan.first().unwrap().1[0].with, "[PERSON_1]");
    assert_eq!(plan.last().unwrap().1[0].with, "[PERSON_1]");
    std::fs::remove_dir_all(&dir).ok();
}
