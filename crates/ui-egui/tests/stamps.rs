//! Add a stamp: the palette and placing stamps on the page.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::stamps_ui::{CustomStamp, MAX_STAMP_BYTES, PresetLayout, StampKind};
use pdfcraft_ui_egui::{PdfCraftApp, QuickTool};

#[test]
fn choosing_and_placing_stamps() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("form.pdf", None, include_bytes!("data/form.pdf").to_vec()).unwrap();
        app.set_option("zoom", "150").unwrap();
        app
    });
    h.run_steps(6);
    assert!(h.state_mut().execute("comment.stamp"));
    h.run_steps(2);
    h.get_by_label("Add a stamp");
    h.get_all_by_label("APPROVED").next().expect("dynamic approved").click();
    h.run_steps(2);
    assert_eq!(h.state().quick_tool, QuickTool::Stamp(pdfcraft_engine::StampKind::DynApproved));
    let p = {
        let r = h.state().views[0].page_screen_rect(0).unwrap();
        r.min + egui::vec2(r.width() * 0.5, r.height() * 0.85)
    };
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(4);
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    let stamps: Vec<_> = doc.info.annotations.iter().filter(|a| a.subtype == "Stamp").collect();
    assert_eq!(stamps.len(), 1);
    assert_eq!(s.quick_tool, QuickTool::Select, "back to selecting after one stamp");
    for _ in 0..30 {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    if let Ok(dir) = std::env::var("PDFCRAFT_SHOTS") {
        h.render().unwrap().save(format!("{dir}/stamps.png")).unwrap();
    }
}

#[test]
fn natural_image_stamps_keep_displayed_bounds_on_rotated_pages() {
    for degrees in [0, 90, 180, 270] {
        let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            app.open_bytes("form.pdf", None, include_bytes!("data/form.pdf").to_vec()).unwrap();
            let id = app.views[0].id;
            app.session.apply(id, pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees }).unwrap();
            app.set_option("zoom", "100").unwrap();
            app
        });
        h.run_steps(6);
        assert!(h.state_mut().execute("comment.stamp"));
        let mut png = Vec::new();
        image::RgbImage::from_pixel(80, 40, image::Rgb([20, 120, 220]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        h.state_mut().start_custom_stamp("rectangle.png".into(), png);
        h.run_steps(2);
        h.get_by_label("OK").click();
        h.run_steps(2);
        let page = h.state().views[0].page_screen_rect(0).unwrap();
        let at = page.min + egui::vec2(page.width() * 0.5, page.height() * 0.6);
        h.hover_at(at);
        h.run_steps(1);
        h.drag_at(at);
        h.run_steps(1);
        h.drop_at(at);
        h.run_steps(4);
        let app = h.state();
        let doc = app.session.get(app.views[0].id).unwrap();
        assert_eq!(app.quick_tool, QuickTool::Select);
        let stamp = doc.info.annotations.iter().find(|a| a.subtype == "Stamp").unwrap();
        let info = &doc.info.pages[0];
        let a = info.user_to_view(stamp.rect[0], stamp.rect[1]);
        let b = info.user_to_view(stamp.rect[2], stamp.rect[3]);
        assert!(((a[0] - b[0]).abs() - 80.0).abs() < 0.001, "rotation {degrees}: {a:?}, {b:?}");
        assert!(((a[1] - b[1]).abs() - 40.0).abs() < 0.001, "rotation {degrees}: {a:?}, {b:?}");
        assert_eq!(doc.can_undo(), Some("Add stamp"));
    }
}

#[test]
fn creating_placing_and_keeping_custom_stamps() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("form.pdf", None, include_bytes!("data/form.pdf").to_vec()).unwrap();
        app.set_option("zoom", "150").unwrap();
        app
    });
    h.run_steps(6);
    assert!(h.state_mut().execute("comment.stamp"));
    h.run_steps(2);
    // An image becomes a stamp: Create ▸ category and name.
    let mut png = Vec::new();
    image::RgbImage::from_pixel(60, 20, image::Rgb([20, 120, 220])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    h.state_mut().start_custom_stamp("logo.png".into(), png);
    h.run_steps(2);
    h.get_by_label("Create Custom Stamp");
    h.state_mut().stamp_draft.category = "Company".into();
    h.run_steps(1);
    h.get_by_label("OK").click();
    h.run_steps(2);
    assert_eq!(h.state().quick_tool, QuickTool::CustomStamp(0));
    h.get_by_label("Company");
    h.get_by_label("logo");
    // Place it.
    let p = {
        let r = h.state().views[0].page_screen_rect(0).unwrap();
        r.min + egui::vec2(r.width() * 0.5, r.height() * 0.85)
    };
    h.hover_at(p);
    h.run_steps(1);
    h.drag_at(p);
    h.run_steps(1);
    h.drop_at(p);
    h.run_steps(4);
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    let stamps: Vec<_> = doc.info.annotations.iter().filter(|a| a.subtype == "Stamp").collect();
    assert_eq!(stamps.len(), 1);
    let r = stamps[0].rect;
    assert!(((r[2] - r[0]) / (r[3] - r[1]) - 3.0).abs() < 0.05, "keeps the picture's shape: {r:?}");
    assert_eq!(doc.can_undo(), Some("Add stamp"));
    // The library is kept with the app's settings.
    let saved = s.persist();
    let mut again = PdfCraftApp::new();
    again.set_option("language", "en").unwrap();
    again.restore(&saved);
    assert_eq!(again.custom_stamps.len(), 1);
    assert_eq!((again.custom_stamps[0].category.as_str(), again.custom_stamps[0].name.as_str()), ("Company", "logo"));
    assert_eq!(again.custom_stamps[0].data, s.custom_stamps[0].data);
}

/// A PNG of `w` × `h` pixels in one colour, with alpha `a`. Synthetic: no real stamp or signature.
fn picture(w: u32, h: u32, a: u8) -> Vec<u8> {
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(w, h, image::Rgba([20, 60, 160, a])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    png
}

fn harness(language: &str) -> Harness<'static, PdfCraftApp> {
    let language = language.to_string();
    // Tall enough for the whole stamps palette, so its last sections can be clicked.
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1800.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", &language).unwrap();
        app.open_bytes("form.pdf", None, include_bytes!("data/form.pdf").to_vec()).unwrap();
        app.set_option("zoom", "100").unwrap();
        app
    });
    h.run_steps(6);
    h
}

/// The screen point of a displayed page point (points from the page's top-left corner).
fn on_page(h: &Harness<'static, PdfCraftApp>, x: f32, y: f32) -> egui::Pos2 {
    let s = h.state();
    let r = s.views[0].page_screen_rect(0).unwrap();
    let page = &s.session.get(s.views[0].id).unwrap().info.pages[0];
    r.min + egui::vec2(x / page.width * r.width(), y / page.height * r.height())
}

fn click(h: &mut Harness<'static, PdfCraftApp>, at: egui::Pos2) {
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    h.run_steps(4);
}

/// Stamp annotations as displayed rectangles [x0, y0, x1, y1] in page points from the top-left.
fn displayed_stamps(h: &Harness<'static, PdfCraftApp>) -> Vec<[f32; 4]> {
    let s = h.state();
    let doc = s.session.get(s.views[0].id).unwrap();
    let page = &doc.info.pages[0];
    doc.info
        .annotations
        .iter()
        .filter(|a| a.subtype == "Stamp")
        .map(|a| {
            let p = page.user_to_view(a.rect[0], a.rect[1]);
            let q = page.user_to_view(a.rect[2], a.rect[3]);
            [p[0].min(q[0]), p[1].min(q[1]), p[0].max(q[0]), p[1].max(q[1])]
        })
        .collect()
}

fn close(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.6)
}

/// A graphic signature slot is created with a name and no picture; it places nothing and the
/// palette says what to provide, until a picture is given. A broken or oversized file is refused
/// and leaves the slot as it was. The library survives a restart with its kinds and sizes.
#[test]
fn signature_slots_wait_for_their_picture_and_say_what_to_provide() {
    let mut h = harness("en");
    assert!(h.state_mut().execute("comment.stamp"));
    h.run_steps(2);
    h.get_by_label("GRAPHIC SIGNATURES");
    h.get_by_label("Add graphic signature…").click();
    h.run_steps(2);
    h.get_by_label("Add Graphic Signature");
    h.get_by_label("No picture yet");
    h.state_mut().stamp_draft.name = "Ada".into();
    h.run_steps(1);
    h.get_by_label("OK").click();
    h.run_steps(2);
    let s = h.state();
    assert_eq!(s.custom_stamps.len(), 1);
    assert_eq!((s.custom_stamps[0].kind, s.custom_stamps[0].configured(), s.custom_stamps[0].category.as_str()), (StampKind::Signature, false, ""));
    assert_eq!(s.quick_tool, QuickTool::Select, "a slot without its picture is not a tool yet");
    h.get_by_label("Ada");
    h.get_by_label("Not configured");
    h.get_by_label("Provide a PNG or JPEG of the original, ideally with a transparent background.");
    // Clicking the page with the slot chosen places nothing and says why.
    h.state_mut().quick_tool = QuickTool::CustomStamp(0);
    h.run_steps(1);
    let at = on_page(&h, 200.0, 300.0);
    click(&mut h, at);
    assert!(displayed_stamps(&h).is_empty());
    assert_eq!(h.state().toast.clone().unwrap().0, "Ada has no picture yet. Choose one in the stamps palette.");
    // A broken file and a file over the limit are refused; the slot is unchanged.
    assert!(h.state_mut().set_stamp_picture(0, "bad.png".into(), b"not a picture".to_vec()).is_err());
    assert!(h.state_mut().set_stamp_picture(0, "huge.png".into(), vec![0; MAX_STAMP_BYTES + 1]).is_err());
    assert!(!h.state().custom_stamps[0].configured());
    // The picture arrives: a 90 × 30 signature. It is placed keeping its 3:1 shape, as the tool.
    h.state_mut().set_stamp_picture(0, "ada.png".into(), picture(90, 30, 255)).unwrap();
    assert_eq!(h.state().custom_stamps[0].size, Some((90.0, 30.0)));
    h.state_mut().quick_tool = QuickTool::CustomStamp(0);
    h.run_steps(2);
    click(&mut h, at);
    let placed = displayed_stamps(&h);
    assert_eq!(placed.len(), 1, "{placed:?}");
    assert!(close(placed[0], [155.0, 285.0, 245.0, 315.0]), "{placed:?}");
    assert_eq!(h.state().quick_tool, QuickTool::Select);
    let doc = h.state().session.get(h.state().views[0].id).unwrap();
    assert_eq!(doc.can_undo(), Some("Add stamp"));
    // The library is kept with the app's settings, kind and size included.
    let saved = h.state().persist();
    let mut again = PdfCraftApp::new();
    again.set_option("language", "en").unwrap();
    again.restore(&saved);
    assert_eq!(again.custom_stamps, h.state().custom_stamps);
}

/// A preset places its stamp at natural size and its signature across it, as one undo step, from
/// one click; it waits while a picture is missing, and it is kept with the settings.
#[test]
fn presets_place_a_stamp_and_a_signature_together_as_one_step() {
    let mut h = harness("en");
    assert!(h.state_mut().execute("comment.stamp"));
    h.run_steps(2);
    // A stamp (120 × 60) through the Create dialog, and a signature slot still without a picture.
    h.state_mut().start_custom_stamp("company.png".into(), picture(120, 60, 255));
    h.run_steps(2);
    h.state_mut().stamp_draft.category = "Office".into();
    h.state_mut().stamp_draft.name = "Company".into();
    h.run_steps(1);
    h.get_by_label("OK").click();
    h.run_steps(2);
    h.state_mut().custom_stamps.push(CustomStamp::slot(StampKind::Signature, "", "Ada"));
    h.run_steps(2);
    // New preset: the first stamp and signature are chosen already; name it and confirm.
    h.get_by_label("New preset…").click();
    h.run_steps(2);
    h.get_by_label("New Preset");
    assert_eq!((h.state().preset_draft.stamp, h.state().preset_draft.signature), (Some(0), Some(1)));
    h.state_mut().preset_draft.name = "Company + Ada".into();
    h.run_steps(1);
    h.get_by_label("OK").click();
    h.run_steps(2);
    assert_eq!(h.state().stamp_presets.len(), 1);
    assert_eq!(h.state().quick_tool, QuickTool::Select, "the signature has no picture yet");
    h.get_by_label("Company + Ada");
    // Clicking the page places nothing while the signature is missing.
    h.state_mut().quick_tool = QuickTool::StampPreset(0);
    h.run_steps(1);
    let at = on_page(&h, 150.0, 250.0);
    click(&mut h, at);
    assert!(displayed_stamps(&h).is_empty());
    assert_eq!(h.state().toast.clone().unwrap().0, "Needs the picture of Ada");
    // With the signature's picture, one click places both: the stamp centred on the click at
    // 120 × 60, the signature 102 pt wide (85 %) in its 3:1 shape, 9 pt (15 %) lower.
    h.state_mut().set_stamp_picture(1, "ada.png".into(), picture(90, 30, 200)).unwrap();
    h.state_mut().quick_tool = QuickTool::StampPreset(0);
    h.run_steps(2);
    click(&mut h, at);
    let placed = displayed_stamps(&h);
    assert_eq!(placed.len(), 2, "{placed:?}");
    assert!(close(placed[0], [90.0, 220.0, 210.0, 280.0]), "{placed:?}");
    assert!(close(placed[1], [99.0, 242.0, 201.0, 276.0]), "{placed:?}");
    assert_eq!(h.state().quick_tool, QuickTool::Select);
    let doc = h.state().session.get(h.state().views[0].id).unwrap();
    assert_eq!(doc.can_undo(), Some("Add stamp and signature"));
    assert_eq!(doc.info.annotations.iter().map(|a| a.contents.clone().unwrap_or_default()).collect::<Vec<_>>(), ["Company", "Ada"]);
    h.state_mut().undo();
    h.run_steps(3);
    assert!(displayed_stamps(&h).is_empty(), "one undo takes both back");
    h.state_mut().redo();
    h.run_steps(3);
    assert_eq!(displayed_stamps(&h).len(), 2);
    // Kept with the settings, layout included; a deleted entry leaves the preset waiting.
    let saved = h.state().persist();
    let mut again = PdfCraftApp::new();
    again.set_option("language", "en").unwrap();
    again.restore(&saved);
    assert_eq!(again.stamp_presets, h.state().stamp_presets);
    assert_eq!(again.stamp_presets[0].layout, PresetLayout::Across);
    again.custom_stamps.retain(|s| s.kind != StampKind::Signature);
    assert!(pdfcraft_ui_egui::stamps_ui::preset_gap(&again.stamp_presets[0], &again.custom_stamps).unwrap().contains("no longer in the library"));
    if let Ok(dir) = std::env::var("PDFCRAFT_SHOTS") {
        // Let the page raster catch up with the redo, so the shot shows both pictures.
        for _ in 0..60 {
            h.run_steps(2);
            std::thread::sleep(std::time::Duration::from_millis(10));
            if !h.state().render_pending() {
                break;
            }
        }
        h.render().unwrap().save(format!("{dir}/stamp-preset.png")).unwrap();
    }
}

/// A placed picture stamp keeps the picture's proportions when a corner handle resizes it, as
/// Fill & Sign image signatures do, and the resize is one undo step.
#[test]
fn picture_stamps_keep_their_proportions_from_the_corner_handles() {
    let mut h = harness("en");
    assert!(h.state_mut().execute("comment.stamp"));
    h.run_steps(2);
    h.state_mut().start_custom_stamp("company.png".into(), picture(120, 60, 255));
    h.run_steps(2);
    h.state_mut().stamp_draft.category = "Office".into();
    h.run_steps(1);
    h.get_by_label("OK").click();
    h.run_steps(2);
    let at = on_page(&h, 200.0, 300.0);
    click(&mut h, at);
    let placed = displayed_stamps(&h);
    assert!(close(placed[0], [140.0, 270.0, 260.0, 330.0]), "{placed:?}");
    // Select it (the form's own widgets come first in /Annots), then drag its bottom-right
    // corner 40 pt right (and only 2 down).
    let index = {
        let s = h.state();
        s.session.get(s.views[0].id).unwrap().info.annotations.iter().find(|a| a.subtype == "Stamp").unwrap().index
    };
    h.state_mut().views[0].comments.selected = Some((0, index));
    for _ in 0..20 {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(h.state().views[0].comments.selected, Some((0, index)), "the stamp stays selected");
    assert_eq!(h.state().quick_tool, QuickTool::Select);
    let corner = on_page(&h, 260.0, 330.0);
    h.drag_at(corner);
    h.run_steps(1);
    let end = on_page(&h, 300.0, 332.0);
    h.hover_at(end);
    h.run_steps(2);
    assert!(
        matches!(h.state().views[0].comments.gesture, Some(pdfcraft_ui_egui::comments::Gesture::Resize { .. })),
        "the corner handle starts a resize: {:?}",
        h.state().views[0].comments.gesture
    );
    h.drop_at(end);
    h.run_steps(3);
    let resized = displayed_stamps(&h)[0];
    let (w, hh) = (resized[2] - resized[0], resized[3] - resized[1]);
    assert!((w / hh - 2.0).abs() < 0.02, "the 2:1 picture keeps its shape: {resized:?}");
    assert!(w > 150.0, "wider than before: {resized:?}");
    assert!((resized[0] - 140.0).abs() < 0.6 && (resized[1] - 270.0).abs() < 0.6, "the opposite corner stays: {resized:?}");
    let doc = h.state().session.get(h.state().views[0].id).unwrap();
    assert_eq!(doc.can_undo(), Some("Resize comment"));
}

/// The Italian interface names the library's sections and states.
#[test]
fn the_italian_palette_names_graphic_signatures_and_presets() {
    let mut h = harness("it");
    assert!(h.state_mut().execute("comment.stamp"));
    h.state_mut().custom_stamps.push(CustomStamp::slot(StampKind::Signature, "", "Firma 1"));
    h.run_steps(3);
    h.get_by_label("FIRME GRAFICHE");
    h.get_by_label("PRESET");
    h.get_by_label("Aggiungi firma grafica…");
    h.get_by_label("Non configurato");
    h.get_by_label("Fornisci un PNG o JPEG dell'originale, preferibilmente con sfondo trasparente.");
    h.get_by_label("Una firma grafica è l'immagine di una firma autografa. Non è una firma digitale certificata: per quella usa Firma digitalmente.");
    if let Ok(dir) = std::env::var("PDFCRAFT_SHOTS") {
        h.render().unwrap().save(format!("{dir}/stamps-it.png")).unwrap();
    }
}

/// The user's library comes first in the palette. PdfCraft's standard stamps are shown while
/// the library is empty, fold away once it has an entry, and unfold on request; the choice is
/// kept with the settings.
#[test]
fn the_library_comes_first_and_the_standard_stamps_fold_away() {
    let mut h = harness("en");
    assert!(h.state_mut().execute("comment.stamp"));
    h.run_steps(2);
    assert!(h.query_all_by_label("APPROVED").count() >= 1, "an empty library leaves the standard stamps open");
    h.state_mut().custom_stamps.push(CustomStamp::slot(StampKind::Signature, "", "Ada"));
    h.run_steps(2);
    assert_eq!(h.query_all_by_label("APPROVED").count(), 0, "folded away once the library has an entry");
    let ada = h.get_by_label("Ada").rect();
    let header = h.get_by_label("STANDARD STAMPS").rect();
    assert!(ada.top() < header.top(), "the library sits above the standard stamps: {ada:?} {header:?}");
    h.get_by_label("STANDARD STAMPS").click();
    h.run_steps(2);
    assert!(h.query_all_by_label("APPROVED").count() >= 1, "unfolded on request");
    assert!(h.get_by_label("Ada").rect().top() < h.get_all_by_label("APPROVED").next().unwrap().rect().top());
    assert_eq!(h.state().standard_stamps_open, Some(true));
    let saved = h.state().persist();
    let mut again = PdfCraftApp::new();
    again.restore(&saved);
    assert_eq!(again.standard_stamps_open, Some(true));
    h.get_by_label("STANDARD STAMPS").click();
    h.run_steps(2);
    assert_eq!(h.query_all_by_label("APPROVED").count(), 0);
    assert_eq!(h.state().standard_stamps_open, Some(false));
}
