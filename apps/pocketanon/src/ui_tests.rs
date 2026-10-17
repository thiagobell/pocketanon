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
    for _ in 0..4000 {
        h.run_steps(1);
        if !h.state().jobs.busy() {
            h.run_steps(2);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("background job did not finish");
}

/// Typing is enough: detection starts after the debounce and the Result follows the Review.
#[test]
fn typing_detects_and_applies_by_itself() {
    use std::time::{Duration, Instant};
    let mut h = Harness::builder().with_size(egui::vec2(1360.0, 840.0)).build_ui_state(|ui, app: &mut PocketAnon| app.show(ui), app());
    wait(&mut h); // model load
    h.get_by_role(egui::accesskit::Role::MultilineTextInput).focus();
    h.run_steps(1);
    h.get_by_role(egui::accesskit::Role::MultilineTextInput).type_text("Please call Max Mustermann at max@example.com today.");
    h.run_steps(2);
    assert!(h.state().text.input.contains("Mustermann"));
    // Within the debounce nothing runs.
    assert!(h.state().text.dirty);
    assert!(h.state().text.detecting.is_none());
    assert!(h.state().text.reviews.is_none());
    // Pretend the last keystroke was long enough ago.
    h.state_mut().text.last_edit = Some(Instant::now() - Duration::from_secs(2));
    h.run_steps(1);
    assert!(h.state().text.detecting.is_some(), "detection should start after the debounce");
    wait(&mut h);
    let reviews = h.state().text.reviews.as_ref().expect("review present");
    assert_eq!(reviews[0].review.entities().len(), 2);
    assert!(h.query_all_by_label_contains("Max Mustermann").next().is_some());
    assert_eq!(h.state().text.output, "Please call [PERSON_1] at [EMAIL_1] today.");
    assert_eq!(h.state().text.summary.as_ref().unwrap().total(), 2);

    // Rejecting a Candidate updates the Result without any button.
    let id = h.state().text.reviews.as_ref().unwrap()[0].review.items()[0].id;
    h.state_mut().text.reviews.as_mut().unwrap()[0].review.set_accepted(id, false);
    h.state_mut().text.output_stale = true;
    h.run_steps(1);
    assert_eq!(h.state().text.output, "Please call Max Mustermann at [EMAIL_1] today.");

    // Editing again hides the stale highlights and re-arms the debounce.
    // The Result pane is a read-only multiline widget too; the editor comes first.
    h.get_all_by_role(egui::accesskit::Role::MultilineTextInput).next().unwrap().type_text(" Thanks.");
    h.run_steps(1);
    assert!(h.state().text.dirty);
    assert!(!h.state().text.review_current());
    h.state_mut().text.last_edit = Some(Instant::now() - Duration::from_secs(2));
    h.run_steps(1);
    wait(&mut h);
    assert!(h.state().text.review_current());
    assert_eq!(h.state().text.output, "Please call [PERSON_1] at [EMAIL_1] today. Thanks.");
}

/// A detection result is bound to the text it was started on, not to whatever is in the editor
/// when it completes.
#[test]
fn late_result_binds_to_its_snapshot() {
    let mut h = Harness::builder().with_size(egui::vec2(1360.0, 840.0)).build_ui_state(|ui, app: &mut PocketAnon| app.show(ui), app());
    wait(&mut h);
    h.state_mut().text.input = "Max Mustermann".into();
    h.state_mut().detect_text();
    h.state_mut().text.input = "XX".into(); // user keeps typing while the job runs
    h.run_steps(1);
    wait(&mut h);
    let r = h.state().text.reviews.as_ref().unwrap();
    assert_eq!(r[0].text, "Max Mustermann");
    assert!(!h.state().text.review_current());
}

/// A Term Rule added from the UI path (context menu / Legend) is detected on the next run.
#[test]
fn term_rule_is_detected_on_next_run() {
    use pocketanon_core::EntityType;
    let mut h = Harness::builder().with_size(egui::vec2(1360.0, 840.0)).build_ui_state(|ui, app: &mut PocketAnon| app.show(ui), app());
    wait(&mut h);
    h.state_mut().text.input = "projekt falke startet morgen".into();
    h.state_mut().detect_text();
    h.run_steps(1);
    wait(&mut h);
    assert!(h.state().text.reviews.as_ref().unwrap()[0].review.entities().is_empty());
    // What the "Redact “falke” as ▸ New type…" menu does.
    h.state_mut().add_term_rule("FALKE", &EntityType::new("project code name"));
    assert!(h.state().text.dirty, "adding a Term Rule re-arms detection immediately");
    h.run_steps(1);
    wait(&mut h);
    let r = &h.state().text.reviews.as_ref().unwrap()[0];
    let ents = r.review.entities();
    assert_eq!(ents.len(), 1);
    assert_eq!(ents[0].text(&r.text), "falke");
    assert_eq!(ents[0].entity_type.as_str(), "project code name");
    assert!(h.state().settings.custom_types.contains(&EntityType::new("project code name")));
    assert_eq!(h.state().text.output, "projekt [PROJECT_CODE_NAME_1] startet morgen");
    // The Legend lists the rule.
    assert!(h.query_all_by_label_contains("FALKE").next().is_some());
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

    // A real DOCX through the Wordcraft adapter: page view with highlights and tags.
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/letter_de.docx");
    let services = Services {
        detectors: vec![Box::new(demo_detector()), Box::new(pocketanon_detect_rules::RuleDetector::new())],
        documents: Box::new(crate::services::real::WordcraftLoader),
        models: Box::new(FakeModelManager),
    };
    let mut h = Harness::builder()
        .with_size(egui::vec2(1360.0, 840.0))
        .with_pixels_per_point(2.0)
        .wgpu()
        .build_ui_state(|ui, app: &mut PocketAnon| app.show(ui), PocketAnon::new(None, services, SettingsStore::in_memory()));
    wait(&mut h);
    h.state_mut().open_path(fixture);
    wait(&mut h);
    h.state_mut().detect_file();
    h.run_steps(1);
    wait(&mut h);
    for _ in 0..6 {
        h.run_steps(2);
        wait(&mut h); // page renders and placements
    }
    shot(&mut h, "08-docx-pages.png");
    h.state_mut().file.zoom = 1.6;
    h.state_mut().file.fit_pending = false;
    for _ in 0..4 {
        h.run_steps(2);
        wait(&mut h);
    }
    shot(&mut h, "09-docx-zoomed.png");
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
    let (plan, summary) = crate::review_ui::plan_all(reviews);
    assert_eq!(summary.total(), 3);
    // Same person in body paragraph 1 and 3 gets the same index across segments.
    assert_eq!(plan.first().unwrap().1[0].with, "[PERSON_1]");
    assert_eq!(plan.last().unwrap().1[0].with, "[PERSON_1]");
    std::fs::remove_dir_all(&dir).ok();
}
