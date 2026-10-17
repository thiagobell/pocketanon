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

/// Opening a document starts detection by itself, even while the Model is still loading.
#[test]
fn opening_a_document_detects_by_itself() {
    let dir = std::env::temp_dir().join(format!("pocketanon-open-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("brief.txt");
    std::fs::write(&input, "hi Max Mustermann, write to max@example.com.").unwrap();
    // Open before the first frame: the Model is still loading when the document arrives.
    let mut a = app();
    assert!(a.model.loading, "the demo Model loads in the background");
    a.open_path(input);
    let mut h = Harness::builder().with_size(egui::vec2(1360.0, 840.0)).build_ui_state(|ui, app: &mut PocketAnon| app.show(ui), a);
    wait(&mut h);
    let r = h.state().file.reviews.as_ref().expect("detected without pressing Detect");
    assert_eq!(r.iter().map(|r| r.review.entities().len()).sum::<usize>(), 2);
    std::fs::remove_dir_all(&dir).ok();
}

/// With no document open, the detection panel is shown even when collapsed.
#[test]
fn detection_panel_is_shown_without_a_document() {
    let mut h = Harness::builder().with_size(egui::vec2(1360.0, 840.0)).build_ui_state(|ui, app: &mut PocketAnon| app.show(ui), app());
    wait(&mut h);
    h.state_mut().legend_open = false;
    h.state_mut().workspace = crate::app::Workspace::File;
    h.run_steps(2);
    assert!(h.query_by_label("What to detect").is_some());
    // In the Text workspace the collapsed state is respected.
    h.state_mut().workspace = crate::app::Workspace::Text;
    h.run_steps(2);
    assert!(h.query_by_label("What to detect").is_none());
}

/// Saving never touches the Review: reject one, save, the Review is still there with the
/// rejection, and saving again writes the same output.
#[test]
fn review_survives_apply_and_save() {
    let dir = std::env::temp_dir().join(format!("pocketanon-save-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("brief.txt");
    std::fs::write(&input, "hi Max Mustermann,\n\nwrite to max@example.com.").unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1360.0, 840.0)).build_ui_state(|ui, app: &mut PocketAnon| app.show(ui), app());
    wait(&mut h);
    h.state_mut().open_path(input.clone());
    wait(&mut h);
    // Reject the email.
    let (seg, id) = {
        let r = h.state().file.reviews.as_ref().unwrap();
        let s = r.iter().find(|s| s.text.contains('@')).unwrap();
        (s.id, s.review.items()[0].id)
    };
    h.state_mut().apply_review_ops(crate::app::Workspace::File, vec![crate::review_ui::ReviewOp::SetAccepted(seg, id, false)]);
    let out = dir.join("brief.anonymized.txt");
    h.state_mut().start_save(out.clone());
    assert!(h.state().file.saving);
    wait(&mut h);
    let first = std::fs::read_to_string(&out).unwrap();
    assert_eq!(first, "hi [PERSON_1],\n\nwrite to max@example.com.");
    let st = h.state();
    assert!(!st.file.saving);
    assert_eq!(st.file.summary.as_ref().unwrap().total(), 1);
    let r = st.file.reviews.as_ref().expect("Review still present after saving");
    let item = r.iter().find(|s| s.id == seg).unwrap().review.get(id).unwrap();
    assert!(!item.accepted, "rejection survives");
    // The working document is untouched.
    assert!(st.file.segments.iter().any(|s| s.text.contains("Max Mustermann")));
    // Re-detecting keeps the decision too.
    h.state_mut().detect_file();
    wait(&mut h);
    let r = h.state().file.reviews.as_ref().unwrap();
    assert!(r.iter().find(|s| s.id == seg).unwrap().review.items().iter().any(|i| i.candidate.text(&r[1].text).contains('@') && !i.accepted));
    // Save again: same output.
    std::fs::remove_file(&out).unwrap();
    h.state_mut().start_save(out.clone());
    wait(&mut h);
    assert_eq!(std::fs::read_to_string(&out).unwrap(), first);
    std::fs::remove_dir_all(&dir).ok();
}

/// The row widgets are clickable: retype through the dropdown, delete through the button.
#[test]
fn retype_and_delete_by_clicking() {
    use egui::accesskit::Role;
    let mut h = Harness::builder().with_size(egui::vec2(1360.0, 840.0)).build_ui_state(|ui, app: &mut PocketAnon| app.show(ui), app());
    wait(&mut h);
    h.state_mut().text.input = "Please call Max Mustermann at max@example.com today.".into();
    h.state_mut().detect_text();
    h.run_steps(1);
    wait(&mut h);
    // Dropdowns in tree order: detection panel (term type), manual-add type, then one per
    // Review row in document order (person, email).
    let combos: Vec<_> = h.get_all_by_role(Role::ComboBox).collect();
    assert_eq!(combos.len(), 4);
    combos[2].click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "organization").click();
    h.run_steps(2);
    let r = &h.state().text.reviews.as_ref().unwrap()[0];
    let types: Vec<&str> = r.review.items().iter().map(|i| i.candidate.entity_type.as_str()).collect();
    assert_eq!(types, vec!["organization", "email"]);
    assert_eq!(h.state().text.output, "Please call [ORGANIZATION_1] at [EMAIL_1] today.");
    // Delete the email.
    h.get_all_by_role_and_label(Role::Button, "🗙").last().unwrap().click();
    h.run_steps(2);
    let r = &h.state().text.reviews.as_ref().unwrap()[0];
    assert_eq!(r.review.items().len(), 1);
    assert_eq!(h.state().text.output, "Please call [ORGANIZATION_1] at max@example.com today.");
    // Re-detection keeps both decisions.
    h.state_mut().text.dirty = true;
    h.state_mut().text.last_edit = None;
    h.run_steps(1);
    wait(&mut h);
    assert_eq!(h.state().text.output, "Please call [ORGANIZATION_1] at max@example.com today.");
}

/// Selecting text shows the floating Redact bar; its menu lists the document's most common
/// types first, then the full tree.
#[test]
fn redact_bar_on_selection() {
    use egui::accesskit::Role;
    let mut h = Harness::builder().with_size(egui::vec2(1360.0, 840.0)).build_ui_state(|ui, app: &mut PocketAnon| app.show(ui), app());
    wait(&mut h);
    let text = "Please call Max Mustermann at max@example.com in Bonn today.";
    h.state_mut().text.input = text.into();
    h.state_mut().detect_text();
    h.run_steps(1);
    wait(&mut h);
    assert!(h.query_by_role_and_label(Role::Button, "Redact").is_none(), "no selection, no bar");
    let start = text.find("Bonn").unwrap();
    h.state_mut().text.selection = Some(pocketanon_core::Span::new(start, start + 4));
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Redact").click();
    h.run_steps(2);
    // Top types (person, email) appear twice: on top and in the tree.
    assert_eq!(h.get_all_by_role_and_label(Role::Button, "person").count(), 2);
    assert_eq!(h.get_all_by_role_and_label(Role::Button, "location").count(), 1);
    // Pick from the top section (the tree lower down may be scrolled out of view).
    h.get_all_by_role_and_label(Role::Button, "person").next().unwrap().click();
    h.run_steps(3);
    assert_eq!(h.state().text.output, "Please call [PERSON_1] at [EMAIL_1] in [PERSON_2] today.");
    assert!(h.state().text.selection.is_none());
    assert!(h.query_by_role_and_label(Role::Button, "Redact").is_none());
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
    // A page selection, as a drag would produce it (hit-tested at both ends of the words).
    {
        let st = h.state_mut();
        let doc = st.file.doc.clone().unwrap();
        let d = doc.lock().unwrap();
        let seg = st.file.segments.iter().find(|s| s.text.contains("Beispielstraße")).unwrap().clone();
        let start = seg.text.find("Beispielstraße").unwrap();
        let span = pocketanon_core::Span::new(start, start + "Beispielstraße 12".len());
        let pls = d.locate(seg.id, span, 1.0);
        let r = pls[0].rects[0];
        let a = d.hit_test(pls[0].page, r.x + 1.0, r.y + r.h / 2.0, 1.0).unwrap();
        let b = d.hit_test(pls[0].page, r.x + r.w - 0.5, r.y + r.h / 2.0, 1.0).unwrap();
        let span = crate::text_util::clamp_selection(&seg.text, (0, a.offset), (0, b.offset));
        let placements = span.map(|s| d.locate(seg.id, s, 1.0)).unwrap_or_default();
        drop(d);
        st.file.page_sel = Some(crate::app::PageSelection { anchor: a, head: b, span, placements });
    }
    shot(&mut h, "08b-docx-selection.png");
    h.state_mut().bar.open = true;
    shot(&mut h, "08d-docx-redact-menu.png");
    h.state_mut().file.page_sel = None;
    h.state_mut().file.saving = true;
    h.state_mut().file.saving_count = 27;
    shot(&mut h, "08c-docx-saving.png");
    h.state_mut().file.saving = false;
    h.state_mut().about_open = true;
    shot(&mut h, "10-about.png");
    h.state_mut().about_open = false;
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

/// The page selection relies on `hit_test` and `locate` sharing one coordinate space at scale
/// 1.0: hit-testing the centre of a located word lands inside it. Also prints the timings of
/// the steps behind Apply & Save on the fixture.
#[test]
fn docx_hit_test_matches_locate_and_timings() {
    use crate::services::DocumentLoader;
    use std::time::Instant;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/letter_de.docx");
    let bytes = std::fs::read(&path).unwrap();
    let loader = crate::services::real::WordcraftLoader;
    let fmt = loader.detect_format(&path).unwrap();
    let t = Instant::now();
    let doc = loader.load(&bytes, fmt).unwrap();
    let load = t.elapsed();
    let segs = doc.segments();
    let seg = segs.iter().find(|s| s.text.contains("Beispielstraße")).expect("fixture text");
    let start = seg.text.find("Beispielstraße").unwrap();
    let span = pocketanon_core::Span::new(start, start + "Beispielstraße".len());
    let t = Instant::now();
    let pls = doc.locate(seg.id, span, 1.0);
    let locate = t.elapsed();
    let t = Instant::now();
    let _ = doc.locate(seg.id, span, 1.0);
    let locate2 = t.elapsed();
    let r = pls[0].rects[0];
    let t = Instant::now();
    let hit = doc.hit_test(pls[0].page, r.x + r.w / 2.0, r.y + r.h / 2.0, 1.0).expect("hit");
    let hit_time = t.elapsed();
    assert_eq!(hit.segment, seg.id);
    assert!(span.range().contains(&hit.offset), "{hit:?} not in {span:?}");
    let t = Instant::now();
    let _ = doc.render_page(0, 2.0);
    let render = t.elapsed();
    // Apply & Save on a fresh copy, as the app does.
    let t = Instant::now();
    let mut copy = loader.load(&bytes, fmt).unwrap();
    let reload = t.elapsed();
    let rep = pocketanon_core::Replacement { span, entity_type: "address".into(), with: "[ADDRESS_1]".into() };
    let t = Instant::now();
    copy.apply(&[pocketanon_core::SegmentEdits { segment: seg.id, replacements: vec![rep] }]).unwrap();
    let apply = t.elapsed();
    let t = Instant::now();
    let out = copy.save().unwrap();
    let save = t.elapsed();
    assert!(!out.is_empty());
    eprintln!(
        "letter_de.docx: load {load:?}, first locate {locate:?}, second locate {locate2:?}, hit_test {hit_time:?}, render page@2x {render:?}, \
         save path: reload {reload:?} + apply {apply:?} + save {save:?}"
    );
}
