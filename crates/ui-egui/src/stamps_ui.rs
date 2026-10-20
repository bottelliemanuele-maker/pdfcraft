//! The stamp library (Add a stamp ▸ Custom): custom stamps made from a PDF page or an image,
//! graphic signatures (pictures of handwritten signatures), and presets that place a stamp with
//! a signature across it in one click. Entries live in the app's settings under a category and a
//! name. An entry may wait for its picture (a slot): the palette says so and what to provide, and
//! nothing is placed from it until the picture is there.
//!
//! A graphic signature is a picture. It is not a certified digital signature (Use a certificate
//! ▸ Digitally sign), and nothing here puts one on a page without a click on that spot.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Align, Color32, CornerRadius, Layout, Rect, Stroke, vec2};
use pdfcraft_engine::{Edit, GroupPicture, MarkFile};
use pdfcraft_render::PageInfo;
use serde::{Deserialize, Serialize};

use crate::theme::{self, Tokens};
use crate::{Dialog, PdfCraftApp, QuickTool, widgets};

/// Stamp files larger than this aren't kept in the library (it lives in the app's settings).
pub const MAX_STAMP_BYTES: usize = 4 << 20;
/// At most this many entries, and this many presets, are read back from settings.
const MAX_ENTRIES: usize = 256;
/// Names and categories read back from settings are cut to this many characters.
const MAX_NAME_CHARS: usize = 120;
/// A preset's signature is this wide, as a share of the stamp's width, when drawn across it.
const ACROSS_WIDTH: f64 = 0.85;
/// How far below the stamp's centre the signature's centre sits when drawn across it, as a share
/// of the stamp's height.
const ACROSS_DROP: f64 = 0.15;
/// The gap between a stamp and a signature placed below or beside it, in points.
const GAP: f64 = 4.0;

/// What a library entry is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StampKind {
    /// A company or office stamp, or any other picture, placed at its natural size.
    #[default]
    Stamp,
    /// A graphic signature: a picture of a handwritten signature.
    Signature,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomStamp {
    pub category: String,
    pub name: String,
    /// The source file's name (its extension tells images from PDFs); empty without a picture.
    pub file: String,
    /// The page of a PDF (0-based).
    #[serde(default)]
    pub page: usize,
    /// The file's bytes; empty for a slot that still waits for its picture.
    #[serde(with = "b64")]
    pub data: Arc<Vec<u8>>,
    #[serde(default)]
    pub kind: StampKind,
    /// The picture's natural size in points (`pdfcraft_engine::picture_size`), read when it was
    /// imported. `None` for slots, for entries saved before it was recorded, and for a recorded
    /// size that is not two positive numbers (the entry is kept; the size is read again).
    #[serde(default, deserialize_with = "lenient_size")]
    pub size: Option<(f64, f64)>,
}

/// A recorded size, or `None` for anything that is not two finite positive numbers.
fn lenient_size<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<(f64, f64)>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    let Some(a) = v.as_array() else { return Ok(None) };
    let (Some(w), Some(h)) = (a.first().and_then(serde_json::Value::as_f64), a.get(1).and_then(serde_json::Value::as_f64)) else { return Ok(None) };
    Ok((a.len() == 2 && w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0).then_some((w, h)))
}

impl CustomStamp {
    /// A labelled slot that waits for its picture.
    pub fn slot(kind: StampKind, category: &str, name: &str) -> Self {
        Self {
            category: category.trim().into(),
            name: name.trim().into(),
            file: String::new(),
            page: 0,
            data: Arc::new(Vec::new()),
            kind,
            size: None,
        }
    }

    /// Whether the entry has its picture.
    pub fn configured(&self) -> bool {
        !self.data.is_empty()
    }

    /// The picture as the engine takes it.
    pub fn mark_file(&self) -> MarkFile {
        MarkFile { name: self.file.clone(), bytes: self.data.clone(), page: self.page }
    }

    /// The picture's natural size in points: recorded at import, else read from the picture now.
    pub fn natural_size(&self) -> Option<(f64, f64)> {
        self.size
            .filter(|(w, h)| w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0)
            .or_else(|| self.configured().then(|| pdfcraft_engine::picture_size(&self.mark_file()).ok()).flatten())
    }
}

mod b64 {
    use std::sync::Arc;

    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Arc<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v.as_slice()))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<Vec<u8>>, D::Error> {
        let s = String::deserialize(d)?;
        // Four base64 characters hold three bytes: refuse the string before decoding a huge one.
        if s.len() > super::MAX_STAMP_BYTES.div_ceil(3) * 4 + 4 {
            return Err(serde::de::Error::custom("the picture is larger than a stamp may be"));
        }
        base64::engine::general_purpose::STANDARD.decode(s).map(Arc::new).map_err(serde::de::Error::custom)
    }
}

/// Where a preset puts the signature relative to the stamp.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetLayout {
    /// Across the lower half of the stamp, drawn over it.
    #[default]
    Across,
    /// Under the stamp.
    Below,
    /// To the right of the stamp.
    Beside,
}

impl PresetLayout {
    pub const ALL: [PresetLayout; 3] = [PresetLayout::Across, PresetLayout::Below, PresetLayout::Beside];

    /// The English label (translated with `tl!` where shown).
    pub fn label(self) -> &'static str {
        match self {
            PresetLayout::Across => "Signature across the stamp",
            PresetLayout::Below => "Signature below the stamp",
            PresetLayout::Beside => "Signature beside the stamp",
        }
    }
}

/// A stamp and a graphic signature placed together with one click.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StampPreset {
    pub name: String,
    /// The name of the entry of kind [`StampKind::Stamp`] it places.
    pub stamp: String,
    /// The name of the entry of kind [`StampKind::Signature`] it places.
    pub signature: String,
    #[serde(default)]
    pub layout: PresetLayout,
}

/// The library entry a preset refers to: the first of that kind with that name.
pub fn entry<'a>(stamps: &'a [CustomStamp], kind: StampKind, name: &str) -> Option<&'a CustomStamp> {
    stamps.iter().find(|s| s.kind == kind && s.name == name)
}

/// What stops a preset from placing anything, worded for the user; `None` when it is ready.
pub fn preset_gap(preset: &StampPreset, stamps: &[CustomStamp]) -> Option<String> {
    let (Some(stamp), Some(signature)) = (entry(stamps, StampKind::Stamp, &preset.stamp), entry(stamps, StampKind::Signature, &preset.signature))
    else {
        return Some(crate::i18n::fmt(tl!("{name}: a stamp or signature it uses is no longer in the library."), &[("name", &preset.name)]));
    };
    [stamp, signature].into_iter().find(|e| !e.configured()).map(|e| crate::i18n::fmt(tl!("Needs the picture of {name}"), &[("name", &e.name)]))
}

/// The pictures a preset places around the clicked point: the stamp at its natural size (at
/// most 200 pt) and the signature sized from it, laid out as the preset says. `Err` says what
/// is missing.
pub fn preset_pictures(preset: &StampPreset, stamps: &[CustomStamp]) -> Result<Vec<GroupPicture>, String> {
    if let Some(gap) = preset_gap(preset, stamps) {
        return Err(gap);
    }
    let unreadable =
        |e: &CustomStamp| crate::i18n::fmt(tl!("Couldn't read {name}: {e}"), &[("name", &e.file), ("e", tl!("the picture has no size"))]);
    let (Some(stamp), Some(signature)) = (entry(stamps, StampKind::Stamp, &preset.stamp), entry(stamps, StampKind::Signature, &preset.signature))
    else {
        return Err(preset_gap(preset, stamps).unwrap_or_default());
    };
    let (sw, sh) = stamp.natural_size().ok_or_else(|| unreadable(stamp))?;
    let (gw, gh) = signature.natural_size().ok_or_else(|| unreadable(signature))?;
    let k = (pdfcraft_engine::stamp_group::NATURAL_MAX_SIDE / sw.max(sh)).min(1.0);
    let (w, h) = (sw * k, sh * k);
    let (width, offset) = match preset.layout {
        PresetLayout::Across => (w * ACROSS_WIDTH, [0.0, h * ACROSS_DROP]),
        PresetLayout::Below => (w, [0.0, h / 2.0 + w * gh / gw / 2.0 + GAP]),
        PresetLayout::Beside => (w, [w / 2.0 + w / 2.0 + GAP, 0.0]),
    };
    Ok(vec![
        GroupPicture { name: stamp.name.clone(), file: stamp.mark_file(), width: None, offset: [0.0, 0.0] },
        GroupPicture { name: signature.name.clone(), file: signature.mark_file(), width: Some(width), offset },
    ])
}

/// The edit a click at `at` (displayed points on `page`) makes with a preset: one undoable step.
pub fn preset_edit(preset: &StampPreset, stamps: &[CustomStamp], page: usize, info: &PageInfo, at: [f64; 2], author: &str) -> Result<Edit, String> {
    let pictures = preset_pictures(preset, stamps)?;
    pdfcraft_engine::stamp_group(page, info, at, &pictures, author).map_err(|e| e.to_string())
}

/// The Create Custom Stamp / Add Graphic Signature dialog: the chosen file (if any yet), its
/// category and name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StampDraft {
    pub file: String,
    pub data: Arc<Vec<u8>>,
    pub size: Option<(f64, f64)>,
    pub category: String,
    pub name: String,
    pub kind: StampKind,
}

/// The New Preset dialog.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PresetDraft {
    pub name: String,
    /// Indices into the library.
    pub stamp: Option<usize>,
    pub signature: Option<usize>,
    pub layout: PresetLayout,
}

/// Small pictures of the library's entries for the palette, decoded once per picture.
#[derive(Default)]
pub struct Thumbnails {
    /// By entry index: the bytes the picture was made from (so a replaced picture is redone),
    /// and the texture, `None` when the file isn't an image the preview can decode (a PDF page).
    cache: HashMap<usize, (Arc<Vec<u8>>, Option<egui::TextureHandle>)>,
}

/// The longest side of a palette thumbnail, in pixels.
const THUMB_SIDE: usize = 160;

impl Thumbnails {
    fn get(&mut self, ctx: &egui::Context, index: usize, data: &Arc<Vec<u8>>) -> Option<egui::TextureHandle> {
        match self.cache.get(&index) {
            Some((bytes, texture)) if Arc::ptr_eq(bytes, data) => texture.clone(),
            _ => {
                let texture = thumbnail(ctx, &format!("stamp-thumb-{index}"), data);
                self.cache.insert(index, (data.clone(), texture.clone()));
                texture
            }
        }
    }
}

/// A small copy of an image for the palette: PNG and JPEG only (the preview's decoder), sampled
/// down to [`THUMB_SIDE`]. Previews only; the picture placed on the page is the file itself.
fn thumbnail(ctx: &egui::Context, key: &str, data: &[u8]) -> Option<egui::TextureHandle> {
    let image = pdfcraft_engine::SignatureImage::from_bytes(data).ok()?;
    let [w, h] = image.size();
    if w == 0 || h == 0 {
        return None;
    }
    let k = (THUMB_SIDE as f64 / w.max(h) as f64).min(1.0);
    let (tw, th) = (((w as f64 * k) as usize).max(1), ((h as f64 * k) as usize).max(1));
    let rgba = image.rgba();
    let mut out = Vec::with_capacity(tw.saturating_mul(th).saturating_mul(4));
    for y in 0..th {
        let sy = (y.saturating_mul(h) / th).min(h.saturating_sub(1));
        for x in 0..tw {
            let sx = (x.saturating_mul(w) / tw).min(w.saturating_sub(1));
            let i = sy.saturating_mul(w).saturating_add(sx).saturating_mul(4);
            out.extend_from_slice(rgba.get(i..i.saturating_add(4)).unwrap_or(&[0, 0, 0, 0]));
        }
    }
    Some(ctx.load_texture(key, egui::ColorImage::from_rgba_unmultiplied([tw, th], &out), egui::TextureOptions::LINEAR))
}

pub(crate) fn create_body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) -> (bool, bool) {
    let categories: Vec<String> = {
        let mut c: Vec<String> = app.custom_stamps.iter().filter(|s| s.kind == StampKind::Stamp).map(|s| s.category.clone()).collect();
        c.sort();
        c.dedup();
        c
    };
    let signature = app.stamp_draft.kind == StampKind::Signature;
    let title = if signature { tl!("Add Graphic Signature") } else { tl!("Create Custom Stamp") };
    ui.label(egui::RichText::new(title).font(theme::semibold(18.0)));
    ui.add_space(8.0);
    if signature {
        ui.label(egui::RichText::new(tl!(SIGNATURE_HINT)).small().color(t.text_muted));
        ui.add_space(4.0);
    }
    let mut choose = false;
    ui.horizontal(|ui| {
        let from = if app.stamp_draft.file.is_empty() {
            tl!("No picture yet").to_string()
        } else {
            crate::i18n::fmt(tl!("From {file}"), &[("file", &app.stamp_draft.file)])
        };
        ui.label(egui::RichText::new(from).color(t.text_muted));
        if widgets::pill_button(ui, tl!("Choose image…"), false).on_hover_text(tl!("From a PDF page or an image")).clicked() {
            choose = true;
        }
    });
    if app.stamp_draft.file.is_empty() {
        ui.label(egui::RichText::new(tl!(PROVIDE_HINT)).small().color(t.text_faint));
    }
    ui.add_space(8.0);
    let d = &mut app.stamp_draft;
    egui::Grid::new("stamp-create").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        if !signature {
            let l = ui.label(tl!("Category:"));
            ui.horizontal(|ui| {
                ui.add(crate::widgets::line(&mut d.category).desired_width(200.0).hint_text(tl!("e.g. My stamps"))).labelled_by(l.id);
                if !categories.is_empty() {
                    egui::ComboBox::from_id_salt("stamp-categories").selected_text("").width(24.0).show_ui(ui, |ui| {
                        for c in &categories {
                            if ui.selectable_label(d.category == *c, c).clicked() {
                                d.category = c.clone();
                            }
                        }
                    });
                }
            });
            ui.end_row();
        }
        let l = ui.label(tl!("Name:"));
        ui.add(crate::widgets::line(&mut d.name).desired_width(200.0)).labelled_by(l.id);
        ui.end_row();
    });
    ui.add_space(12.0);
    let ok = (signature || !d.category.trim().is_empty()) && !d.name.trim().is_empty();
    let (mut save, mut cancel) = (false, false);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if ui.add_enabled_ui(ok, |ui| widgets::pill_button(ui, tl!("OK"), true)).inner.clicked() {
            save = true;
        }
        if widgets::pill_button(ui, tl!("Cancel"), false).clicked() {
            cancel = true;
        }
    });
    if choose {
        app.pick_stamp_file();
    }
    (save, cancel)
}

/// The New Preset dialog: a name, a stamp, a signature and where the signature goes.
pub(crate) fn preset_body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) -> (bool, bool) {
    ui.label(egui::RichText::new(tl!("New Preset")).font(theme::semibold(18.0)));
    ui.add_space(8.0);
    ui.label(egui::RichText::new(tl!(PRESET_HINT)).small().color(t.text_muted));
    ui.add_space(8.0);
    let stamps = &app.custom_stamps;
    let d = &mut app.preset_draft;
    for (chosen, kind) in [(&mut d.stamp, StampKind::Stamp), (&mut d.signature, StampKind::Signature)] {
        if chosen.is_some_and(|i| stamps.get(i).is_none_or(|s| s.kind != kind)) {
            *chosen = None;
        }
    }
    egui::Grid::new("preset-create").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        let l = ui.label(tl!("Name:"));
        ui.add(crate::widgets::line(&mut d.name).desired_width(220.0)).labelled_by(l.id);
        ui.end_row();
        for (label, chosen, kind, salt) in [
            (tl!("Stamp:"), &mut d.stamp, StampKind::Stamp, "preset-stamp"),
            (tl!("Signature:"), &mut d.signature, StampKind::Signature, "preset-signature"),
        ] {
            ui.label(label);
            let shown = chosen.and_then(|i| stamps.get(i)).map(entry_label).unwrap_or_else(|| "—".into());
            egui::ComboBox::from_id_salt(salt).selected_text(shown).width(220.0).show_ui(ui, |ui| {
                for (i, s) in stamps.iter().enumerate().filter(|(_, s)| s.kind == kind) {
                    if ui.selectable_label(*chosen == Some(i), entry_label(s)).clicked() {
                        *chosen = Some(i);
                    }
                }
            });
            ui.end_row();
        }
        ui.label(tl!("Layout:"));
        ui.vertical(|ui| {
            for layout in PresetLayout::ALL {
                widgets::radio_value(ui, &mut d.layout, layout, tl!(layout.label()));
            }
        });
        ui.end_row();
    });
    ui.add_space(12.0);
    let ok = !d.name.trim().is_empty() && d.stamp.is_some() && d.signature.is_some();
    let (mut save, mut cancel) = (false, false);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if ui.add_enabled_ui(ok, |ui| widgets::pill_button(ui, tl!("OK"), true)).inner.clicked() {
            save = true;
        }
        if widgets::pill_button(ui, tl!("Cancel"), false).clicked() {
            cancel = true;
        }
    });
    (save, cancel)
}

/// An entry's name, marked when it still waits for its picture.
fn entry_label(s: &CustomStamp) -> String {
    if s.configured() { s.name.clone() } else { format!("{} ({})", s.name, tl!("Not configured")) }
}

const SIGNATURE_HINT: &str =
    "A graphic signature is a picture of a handwritten signature. It is not a certified digital signature: for that, use Digitally sign.";
const PROVIDE_HINT: &str = "Provide a PNG or JPEG of the original, ideally with a transparent background.";
const PRESET_HINT: &str = "A preset places a stamp and a graphic signature together with one click.";

/// What the palette asks of the user for one entry.
enum RowAction {
    Pick,
    Choose(usize),
    Delete(usize),
}

/// One library entry in the palette: its thumbnail, name and state; clicking a configured entry
/// picks it as the tool, clicking a slot asks for its picture.
fn entry_row(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens, i: usize) -> Option<RowAction> {
    let s = app.custom_stamps.get(i).cloned()?;
    let active = app.quick_tool == QuickTool::CustomStamp(i);
    let mut action = None;
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(56.0, 28.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect(rect, CornerRadius::same(4), Color32::WHITE, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        if s.configured() {
            match app.stamp_thumbs.get(ui.ctx(), i, &s.data) {
                Some(texture) => {
                    let [w, h] = texture.size();
                    let inner = rect.shrink(3.0);
                    let k = (inner.width() / w.max(1) as f32).min(inner.height() / h.max(1) as f32);
                    let size = vec2(w as f32 * k, h as f32 * k);
                    painter.image(
                        texture.id(),
                        Rect::from_center_size(rect.center(), size),
                        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                None => {
                    painter.text(rect.center(), egui::Align2::CENTER_CENTER, "PDF", theme::semibold(10.0), t.text_muted);
                }
            }
        } else {
            painter.rect_stroke(rect.shrink(4.0), CornerRadius::same(3), Stroke::new(1.0, t.text_faint), egui::StrokeKind::Inside);
        }
        ui.vertical(|ui| {
            let resp = ui.add(egui::Button::selectable(active, s.name.as_str()).min_size(vec2(ui.available_width(), 24.0)));
            if resp.clicked() {
                action = Some(if s.configured() { RowAction::Pick } else { RowAction::Choose(i) });
            }
            if !s.configured() {
                ui.label(egui::RichText::new(tl!("Not configured")).small().color(t.text_muted));
            }
            resp.context_menu(|ui| {
                let choose = if s.configured() { tl!("Replace Image…") } else { tl!("Choose image…") };
                if ui.button(choose).clicked() {
                    action = Some(RowAction::Choose(i));
                    ui.close();
                }
                let delete = if s.kind == StampKind::Signature { tl!("Delete signature") } else { tl!("Delete stamp") };
                if ui.button(delete).clicked() {
                    action = Some(RowAction::Delete(i));
                    ui.close();
                }
            });
        });
    });
    action
}

/// The library sections of the stamps palette: graphic signatures, custom stamps by category,
/// and presets.
pub(crate) fn palette_section(app: &mut PdfCraftApp, ui: &mut egui::Ui, t: &Tokens) {
    let mut action = None;
    let mut remove_preset = None;
    // Graphic signatures.
    widgets::section_title(ui, tl!("Graphic signatures"));
    ui.label(egui::RichText::new(tl!(SIGNATURE_HINT)).small().color(t.text_faint));
    let signatures: Vec<usize> = (0..app.custom_stamps.len()).filter(|i| app.custom_stamps[*i].kind == StampKind::Signature).collect();
    for i in &signatures {
        if let Some(a) = entry_row(ui, app, t, *i) {
            action = Some((*i, a));
        }
    }
    if signatures.iter().any(|i| !app.custom_stamps[*i].configured()) {
        ui.label(egui::RichText::new(tl!(PROVIDE_HINT)).small().color(t.text_faint));
    }
    if widgets::ghost_button(ui, "plus", tl!("Add graphic signature…")).clicked() {
        app.start_stamp_slot(StampKind::Signature);
    }
    ui.add_space(4.0);
    // Custom stamps, by category.
    widgets::section_title(ui, tl!("Custom"));
    let mut categories: Vec<String> = app.custom_stamps.iter().filter(|s| s.kind == StampKind::Stamp).map(|s| s.category.clone()).collect();
    categories.sort();
    categories.dedup();
    for c in categories {
        ui.label(egui::RichText::new(&c).small().color(t.text_muted));
        let rows: Vec<usize> =
            (0..app.custom_stamps.len()).filter(|i| app.custom_stamps[*i].kind == StampKind::Stamp && app.custom_stamps[*i].category == c).collect();
        for i in rows {
            if let Some(a) = entry_row(ui, app, t, i) {
                action = Some((i, a));
            }
        }
    }
    if app.custom_stamps.iter().any(|s| s.kind == StampKind::Stamp && !s.configured()) {
        ui.label(egui::RichText::new(tl!(PROVIDE_HINT)).small().color(t.text_faint));
    }
    ui.add_space(4.0);
    if widgets::pill_button(ui, tl!("Create custom stamp…"), false).on_hover_text(tl!("From a PDF page or an image")).clicked() {
        app.start_stamp_slot(StampKind::Stamp);
    }
    ui.add_space(4.0);
    // Presets.
    widgets::section_title(ui, tl!("Presets"));
    ui.label(egui::RichText::new(tl!(PRESET_HINT)).small().color(t.text_faint));
    for i in 0..app.stamp_presets.len() {
        let Some(preset) = app.stamp_presets.get(i).cloned() else { break };
        let gap = preset_gap(&preset, &app.custom_stamps);
        let active = app.quick_tool == QuickTool::StampPreset(i);
        let resp = ui.add(egui::Button::selectable(active, preset.name.as_str()).min_size(vec2(ui.available_width(), 28.0)));
        let parts = format!("{} · {}", preset.stamp, preset.signature);
        ui.label(egui::RichText::new(parts).small().color(t.text_muted));
        if let Some(gap) = &gap {
            ui.label(egui::RichText::new(format!("{} · {gap}", tl!("Not configured"))).small().color(t.text_muted));
        }
        if resp.clicked() {
            match gap {
                None => app.quick_tool = QuickTool::StampPreset(i),
                Some(why) => app.notify(why),
            }
        }
        resp.context_menu(|ui| {
            if ui.button(tl!("Delete preset")).clicked() {
                remove_preset = Some(i);
                ui.close();
            }
        });
    }
    let can_preset = app.custom_stamps.iter().any(|s| s.kind == StampKind::Stamp) && app.custom_stamps.iter().any(|s| s.kind == StampKind::Signature);
    if ui.add_enabled_ui(can_preset, |ui| widgets::ghost_button(ui, "plus", tl!("New preset…"))).inner.clicked() {
        app.start_stamp_preset();
    }
    match action {
        Some((i, RowAction::Pick)) => app.quick_tool = QuickTool::CustomStamp(i),
        Some((_, RowAction::Choose(i))) => app.pick_stamp_picture(i),
        Some((_, RowAction::Delete(i))) => app.delete_custom_stamp(i),
        None => {}
    }
    if let Some(i) = remove_preset
        && i < app.stamp_presets.len()
    {
        app.stamp_presets.remove(i);
        if matches!(app.quick_tool, QuickTool::StampPreset(_)) {
            app.quick_tool = QuickTool::Select;
        }
    }
}

/// Read a picture file for the library: its bytes within [`MAX_STAMP_BYTES`], and its natural
/// size, which also proves the file is an image or a PDF the engine can place.
/// A picture read for the library: its bytes and its natural size in points.
type Picture = (Arc<Vec<u8>>, (f64, f64));

fn read_picture(file: &str, bytes: Vec<u8>) -> Result<Picture, String> {
    if bytes.len() > MAX_STAMP_BYTES {
        return Err(crate::i18n::fmt(
            tl!("{file} is too large for a stamp (at most {n} MB)"),
            &[("file", file), ("n", &(MAX_STAMP_BYTES >> 20).to_string())],
        ));
    }
    let data = Arc::new(bytes);
    let size = pdfcraft_engine::picture_size(&MarkFile { name: file.into(), bytes: data.clone(), page: 0 })
        .map_err(|e| crate::i18n::fmt(tl!("Couldn't read {name}: {e}"), &[("name", file), ("e", &e.to_string())]))?;
    Ok((data, size))
}

impl PdfCraftApp {
    /// Ask for a PDF or an image, then hand its path to `then`. Tests and automation set
    /// `save_override` to a file instead of showing the panel.
    #[cfg(not(target_arch = "wasm32"))]
    fn ask_picture_file(&mut self, then: impl FnOnce(&mut Self, std::path::PathBuf) + Send + 'static) {
        match self.save_override.clone() {
            Some(p) if [".png", ".jpg", ".jpeg", ".pdf"].iter().any(|e| p.to_ascii_lowercase().ends_with(e)) => then(self, p.into()),
            Some(_) => {}
            None => {
                let dialog = rfd::AsyncFileDialog::new()
                    .add_filter(tl!("PDF or image"), &["pdf", "png", "jpg", "jpeg", "tif", "tiff", "gif", "bmp", "jp2", "j2k", "jpx"])
                    .set_title(tl!("Select a file for the stamp"));
                self.ask_one(crate::pickers::Ask::File(dialog), None, then);
            }
        }
    }

    /// The Create Custom Stamp dialog's Choose image: the file goes into the draft, which keeps
    /// waiting for a name. Unreadable files are reported and leave the draft as it was.
    pub(crate) fn pick_stamp_file(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.ask_picture_file(|app, path| {
            if app.dialog != Some(Dialog::CreateStamp) {
                return;
            }
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            match std::fs::read(&path)
                .map_err(|e| crate::i18n::fmt(tl!("Couldn't read {name}: {e}"), &[("name", &path.display().to_string()), ("e", &e.to_string())]))
            {
                Ok(bytes) => app.set_draft_picture(name, bytes),
                Err(e) => app.notify(e),
            }
        });
        #[cfg(target_arch = "wasm32")]
        self.notify_tr("Custom stamps arrive on the web with file pickers for images");
    }

    /// Choose (or replace) the picture of the library entry `index`, in place.
    pub(crate) fn pick_stamp_picture(&mut self, index: usize) {
        #[cfg(not(target_arch = "wasm32"))]
        self.ask_picture_file(move |app, path| {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let result = std::fs::read(&path)
                .map_err(|e| crate::i18n::fmt(tl!("Couldn't read {name}: {e}"), &[("name", &path.display().to_string()), ("e", &e.to_string())]))
                .and_then(|bytes| app.set_stamp_picture(index, name, bytes));
            if let Err(e) = result {
                app.notify(e);
            }
        });
        #[cfg(target_arch = "wasm32")]
        {
            let _ = index;
            self.notify_tr("Custom stamps arrive on the web with file pickers for images");
        }
    }

    /// Put `bytes` (a PDF or an image file called `file`) into the draft of the Create dialog.
    /// A file that is too large or can't be read is reported and the draft keeps its picture.
    pub fn set_draft_picture(&mut self, file: String, bytes: Vec<u8>) {
        match read_picture(&file, bytes) {
            Ok((data, size)) => {
                if self.stamp_draft.name.trim().is_empty() {
                    self.stamp_draft.name = file.rsplit_once('.').map_or(file.as_str(), |(s, _)| s).to_string();
                }
                self.stamp_draft.file = file;
                self.stamp_draft.data = data;
                self.stamp_draft.size = Some(size);
            }
            Err(e) => self.notify(e),
        }
    }

    /// Give the library entry `index` the picture `bytes` (a PDF or an image file called
    /// `file`). A file that is too large or can't be read as a picture changes nothing.
    pub fn set_stamp_picture(&mut self, index: usize, file: String, bytes: Vec<u8>) -> Result<(), String> {
        let (data, size) = read_picture(&file, bytes)?;
        let entry = self.custom_stamps.get_mut(index).ok_or_else(|| tl!("Not configured").to_string())?;
        entry.file = file;
        entry.data = data;
        entry.page = 0;
        entry.size = Some(size);
        Ok(())
    }

    /// Open the Create dialog for a new entry of `kind`, with no picture yet.
    pub fn start_stamp_slot(&mut self, kind: StampKind) {
        let category = match kind {
            StampKind::Stamp => {
                self.custom_stamps.iter().rev().find(|s| s.kind == StampKind::Stamp).map(|s| s.category.clone()).unwrap_or_else(|| "My stamps".into())
            }
            StampKind::Signature => String::new(),
        };
        self.stamp_draft = StampDraft { category, kind, ..Default::default() };
        self.dialog = Some(Dialog::CreateStamp);
    }

    /// Open the Create Custom Stamp dialog for a file already read (drag and drop, tests).
    pub fn start_custom_stamp(&mut self, file: String, bytes: Vec<u8>) {
        self.start_stamp_slot(StampKind::Stamp);
        self.set_draft_picture(file, bytes);
        if self.stamp_draft.data.is_empty() {
            self.dialog = None;
        }
    }

    /// Save the dialog's entry in the library and, when it has its picture, choose it as the tool.
    pub(crate) fn save_custom_stamp(&mut self) {
        let d = std::mem::take(&mut self.stamp_draft);
        let category = if d.kind == StampKind::Signature { String::new() } else { d.category.trim().into() };
        self.custom_stamps.push(CustomStamp {
            category,
            name: d.name.trim().into(),
            file: d.file,
            page: 0,
            data: d.data,
            kind: d.kind,
            size: d.size,
        });
        let i = self.custom_stamps.len() - 1;
        self.quick_tool = if self.custom_stamps[i].configured() { QuickTool::CustomStamp(i) } else { QuickTool::Select };
    }

    /// Remove the entry `index` from the library (its presets keep its name and say so).
    pub(crate) fn delete_custom_stamp(&mut self, index: usize) {
        if index < self.custom_stamps.len() {
            self.custom_stamps.remove(index);
            self.stamp_thumbs.cache.clear();
            if matches!(self.quick_tool, QuickTool::CustomStamp(_) | QuickTool::StampPreset(_)) {
                self.quick_tool = QuickTool::Select;
            }
        }
    }

    /// Open the New Preset dialog, with the first stamp and signature of the library chosen.
    pub fn start_stamp_preset(&mut self) {
        let first = |kind: StampKind| self.custom_stamps.iter().position(|s| s.kind == kind);
        self.preset_draft = PresetDraft {
            name: String::new(),
            stamp: first(StampKind::Stamp),
            signature: first(StampKind::Signature),
            layout: PresetLayout::default(),
        };
        self.dialog = Some(Dialog::CreatePreset);
    }

    /// Save the dialog's preset and, when its pictures are there, choose it as the tool.
    pub(crate) fn save_stamp_preset(&mut self) {
        let d = std::mem::take(&mut self.preset_draft);
        let (Some(stamp), Some(signature)) = (d.stamp.and_then(|i| self.custom_stamps.get(i)), d.signature.and_then(|i| self.custom_stamps.get(i)))
        else {
            return;
        };
        let preset = StampPreset { name: d.name.trim().into(), stamp: stamp.name.clone(), signature: signature.name.clone(), layout: d.layout };
        let ready = preset_gap(&preset, &self.custom_stamps).is_none();
        self.stamp_presets.push(preset);
        self.quick_tool = if ready { QuickTool::StampPreset(self.stamp_presets.len() - 1) } else { QuickTool::Select };
    }
}

/// The custom stamps for `persist` (kept small: base64 of each file).
pub(crate) fn encode(stamps: &[CustomStamp]) -> serde_json::Value {
    serde_json::to_value(stamps).unwrap_or_default()
}

fn cut(s: &str) -> String {
    s.chars().take(MAX_NAME_CHARS).collect::<String>().trim().to_string()
}

/// Read stamps back; malformed or oversized entries are dropped, names are cut to size.
pub(crate) fn decode(v: &serde_json::Value) -> Vec<CustomStamp> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| serde_json::from_value::<CustomStamp>(x.clone()).ok())
                .filter(|s| s.data.len() <= MAX_STAMP_BYTES)
                .map(|mut s| {
                    s.name = cut(&s.name);
                    s.category = cut(&s.category);
                    s.file = cut(&s.file);
                    s.size = s.size.filter(|(w, h)| w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0);
                    s
                })
                .filter(|s| !s.name.is_empty())
                .take(MAX_ENTRIES)
                .collect()
        })
        .unwrap_or_default()
}

/// The presets for `persist`.
pub(crate) fn encode_presets(presets: &[StampPreset]) -> serde_json::Value {
    serde_json::to_value(presets).unwrap_or_default()
}

/// Read presets back; malformed entries are dropped.
pub(crate) fn decode_presets(v: &serde_json::Value) -> Vec<StampPreset> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| serde_json::from_value::<StampPreset>(x.clone()).ok())
                .map(|mut p| {
                    p.name = cut(&p.name);
                    p.stamp = cut(&p.stamp);
                    p.signature = cut(&p.signature);
                    p
                })
                .filter(|p| !p.name.is_empty())
                .take(MAX_ENTRIES)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(w, h, image::Rgba([20, 60, 160, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn configured(kind: StampKind, name: &str, w: u32, h: u32) -> CustomStamp {
        let mut s = CustomStamp::slot(kind, "Office", name);
        s.file = format!("{name}.png");
        s.data = Arc::new(png(w, h));
        s.size = Some((f64::from(w), f64::from(h)));
        s
    }

    #[test]
    fn presets_lay_the_signature_out_from_the_stamps_size() {
        let stamps = [configured(StampKind::Stamp, "Company", 400, 200), configured(StampKind::Signature, "Ada", 90, 30)];
        let mut preset = StampPreset { name: "Company + Ada".into(), stamp: "Company".into(), signature: "Ada".into(), layout: PresetLayout::Across };
        // The stamp is capped at 200 pt on its longest side: 200 x 100 as displayed.
        let pictures = preset_pictures(&preset, &stamps).unwrap();
        assert_eq!((pictures[0].width, pictures[0].offset), (None, [0.0, 0.0]));
        assert_eq!((pictures[1].width, pictures[1].offset), (Some(170.0), [0.0, 15.0]));
        preset.layout = PresetLayout::Below;
        let pictures = preset_pictures(&preset, &stamps).unwrap();
        // 200 wide → 66.67 tall; its centre sits half the stamp, half itself and a gap lower.
        assert_eq!(pictures[1].width, Some(200.0));
        assert!((pictures[1].offset[1] - (50.0 + 200.0 / 3.0 / 2.0 + GAP)).abs() < 1e-9, "{:?}", pictures[1].offset);
        preset.layout = PresetLayout::Beside;
        let pictures = preset_pictures(&preset, &stamps).unwrap();
        assert_eq!(pictures[1].offset, [200.0 + GAP, 0.0]);
    }

    #[test]
    fn a_preset_waits_for_its_pictures_and_notices_a_deleted_entry() {
        let mut stamps = vec![configured(StampKind::Stamp, "Company", 120, 60), CustomStamp::slot(StampKind::Signature, "", "Ada")];
        let preset = StampPreset { name: "P".into(), stamp: "Company".into(), signature: "Ada".into(), layout: PresetLayout::Across };
        assert_eq!(preset_gap(&preset, &stamps).as_deref(), Some("Needs the picture of Ada"));
        assert!(preset_pictures(&preset, &stamps).is_err());
        stamps[1] = configured(StampKind::Signature, "Ada", 90, 30);
        assert_eq!(preset_gap(&preset, &stamps), None);
        // A stamp of that name but of the other kind does not count.
        stamps[0].kind = StampKind::Signature;
        assert!(preset_gap(&preset, &stamps).unwrap().contains("no longer in the library"));
    }

    #[test]
    fn settings_round_trip_and_drop_what_is_malformed() {
        let stamps = vec![configured(StampKind::Stamp, "Company", 120, 60), CustomStamp::slot(StampKind::Signature, "", "Ada")];
        let back = decode(&encode(&stamps));
        assert_eq!(back, stamps);
        let presets = vec![StampPreset { name: "P".into(), stamp: "Company".into(), signature: "Ada".into(), layout: PresetLayout::Beside }];
        assert_eq!(decode_presets(&encode_presets(&presets)), presets);
        // Older settings: entries without kind or size, which read as stamps of unknown size.
        let old = serde_json::json!([{ "category": "C", "name": "logo", "file": "logo.png", "data": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png(8, 4)) }]);
        let read = decode(&old);
        assert_eq!((read.len(), read[0].kind, read[0].size), (1, StampKind::Stamp, None));
        assert_eq!(read[0].natural_size(), Some((8.0, 4.0)), "the size is read from the picture");
        // Malformed entries: bad base64, a huge picture, a missing name, a wrong type, junk.
        let huge = "A".repeat(MAX_STAMP_BYTES.div_ceil(3) * 4 + 8);
        let bad = serde_json::json!([
            { "category": "C", "name": "x", "file": "x.png", "data": "%%%not base64%%%" },
            { "category": "C", "name": "y", "file": "y.png", "data": huge },
            { "category": "C", "file": "z.png", "data": "" },
            { "category": "C", "name": "   ", "file": "", "data": "" },
            { "category": "C", "name": "ok", "file": "", "data": "", "kind": "signature", "size": ["big", 1.0] },
            { "category": "C", "name": "flat", "file": "", "data": "", "kind": "signature", "size": [0, 5] },
            42,
            "junk",
        ]);
        let read = decode(&bad);
        assert_eq!(read.len(), 2, "a wrong size is dropped, not the entry: {read:?}");
        assert_eq!((read[0].name.as_str(), read[0].kind, read[0].size, read[0].configured()), ("ok", StampKind::Signature, None, false));
        assert_eq!((read[1].name.as_str(), read[1].size), ("flat", None));
        assert!(decode(&serde_json::json!("nonsense")).is_empty());
        assert!(decode_presets(&serde_json::json!([{ "name": "P" }, { "name": "", "stamp": "a", "signature": "b" }, 7])).is_empty());
        let long = "n".repeat(500);
        let read = decode_presets(&serde_json::json!([{ "name": long, "stamp": "a", "signature": "b", "layout": "sideways" }]));
        assert!(read.is_empty(), "an unknown layout is malformed: {read:?}");
        let read = decode_presets(&serde_json::json!([{ "name": long, "stamp": "a", "signature": "b" }]));
        assert_eq!(read[0].name.chars().count(), MAX_NAME_CHARS);
    }
}
