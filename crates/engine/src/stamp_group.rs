//! Stamp groups: several pictures placed on a page together as one undoable step, for a company
//! stamp with a graphic signature across it. Shared by the stamps palette's presets and the
//! `stamp_group` automation tool.
//!
//! A graphic signature is a picture of a handwritten signature. It is not a certified digital
//! signature (`crate::sign`), and nothing here places one without a request for that spot.

use pdfcraft_render::PageInfo;

use crate::{Edit, EditError, MarkFile};

/// What a group is called in the history ("Undo Add stamp and signature"); in [`crate::BATCH_LABELS`].
pub const STAMP_GROUP_LABEL: &str = "Add stamp and signature";

/// The longest side of a picture placed at its natural size, in points, as a point-sized
/// [`Edit::AddCustomStamp`] rectangle is drawn.
pub const NATURAL_MAX_SIDE: f64 = 200.0;

/// A placed width, in displayed points, must lie in this range.
pub const WIDTH_RANGE: std::ops::RangeInclusive<f64> = 1.0..=10_000.0;

/// How far a picture's centre may lie from the group's anchor, in points.
const MAX_OFFSET: f64 = 100_000.0;

/// At most this many pictures in one group.
pub const MAX_PICTURES: usize = 8;

/// One picture of a group.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupPicture {
    /// The stamp's name (its `/Contents`, and the suffix of its `/Name`).
    pub name: String,
    /// The picture: an image file, or a page of a PDF.
    pub file: MarkFile,
    /// Its width as displayed, in points; `None` places it at its natural size, at most
    /// [`NATURAL_MAX_SIDE`] on the longest side. The height follows the picture's proportions.
    pub width: Option<f64>,
    /// Where its centre is, in displayed points right and down from the group's anchor.
    pub offset: [f64; 2],
}

/// The natural size of a picture in points, as [`Edit::AddCustomStamp`] draws it: a PDF page's
/// displayed size (its crop box turned by `/Rotate`), or an image's size at its resolution. The
/// file is read the same way, so the same files are refused.
pub fn picture_size(file: &MarkFile) -> Result<(f64, f64), EditError> {
    if crate::looks_like_pdf(&file.bytes) {
        let src = pdfcraft_cos::Document::open(file.bytes.clone()).map_err(|e| EditError::Source(format!("{}: {e}", file.name)))?;
        Ok(pdfcraft_organize::page_display_size(&src, file.page)?)
    } else {
        Ok(pdfcraft_create::image_size(&file.name, &file.bytes)?)
    }
}

/// `pictures` placed on `page` as one undoable step, anchored at `at` (displayed points from the
/// page's top-left corner; each picture's `offset` is added to it). Every picture keeps its
/// proportions and reads upright as the page is displayed, like a custom stamp placed by hand,
/// and Undo takes the whole group back.
///
/// Refused, changing nothing: no pictures or more than [`MAX_PICTURES`], an anchor or offset
/// that is not a finite number within [`MAX_OFFSET`], a width outside [`WIDTH_RANGE`], or a file
/// that can't be read as an image or a PDF page.
pub fn stamp_group(page: usize, info: &PageInfo, at: [f64; 2], pictures: &[GroupPicture], author: &str) -> Result<Edit, EditError> {
    if pictures.is_empty() {
        return Err(EditError::Source("a stamp group needs at least one picture".into()));
    }
    if pictures.len() > MAX_PICTURES {
        return Err(EditError::Source(format!("a stamp group holds at most {MAX_PICTURES} pictures, not {}", pictures.len())));
    }
    if !at.iter().all(|v| v.is_finite() && v.abs() <= MAX_OFFSET) {
        return Err(EditError::Source("the group's position must be a finite point on the page".into()));
    }
    let mut edits = Vec::with_capacity(pictures.len());
    for p in pictures {
        if !p.offset.iter().all(|v| v.is_finite() && v.abs() <= MAX_OFFSET) {
            return Err(EditError::Source(format!("{}: the offset must be two finite numbers", p.name)));
        }
        let (sw, sh) = picture_size(&p.file)?;
        if !(sw.is_finite() && sh.is_finite() && sw > 0.0 && sh > 0.0) {
            return Err(EditError::Source(format!("{}: the picture has no size", p.file.name)));
        }
        let (w, h) = match p.width {
            Some(w) if w.is_finite() && WIDTH_RANGE.contains(&w) => (w, w * sh / sw),
            Some(w) => {
                return Err(EditError::Source(format!(
                    "{}: the width must be between {} and {} points, not {w}",
                    p.name,
                    WIDTH_RANGE.start(),
                    WIDTH_RANGE.end()
                )));
            }
            None => {
                let k = (NATURAL_MAX_SIDE / sw.max(sh)).min(1.0);
                (sw * k, sh * k)
            }
        };
        if !(h.is_finite() && h > 0.0) {
            return Err(EditError::Source(format!("{}: the picture's proportions give it no height", p.name)));
        }
        let (cx, cy) = (at[0] + p.offset[0], at[1] + p.offset[1]);
        // Two opposite corners as displayed → user space; the page's turn may swap or flip them.
        let a = to_user(info, cx - w / 2.0, cy - h / 2.0);
        let b = to_user(info, cx + w / 2.0, cy + h / 2.0);
        let rect = [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])];
        edits.push(Edit::AddCustomStamp { page, rect, name: p.name.clone(), file: p.file.clone(), author: author.to_string() });
    }
    Ok(Edit::Batch { label: STAMP_GROUP_LABEL.into(), edits })
}

fn to_user(info: &PageInfo, x: f64, y: f64) -> [f64; 2] {
    let [ux, uy] = info.view_to_user(x as f32, y as f32);
    [f64::from(ux), f64::from(uy)]
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;

    use image::ImageFormat;

    use super::*;
    use crate::Session;

    fn png(w: u32, h: u32) -> Arc<Vec<u8>> {
        let image = image::RgbaImage::from_pixel(w, h, image::Rgba([20, 60, 160, 200]));
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, ImageFormat::Png).unwrap();
        Arc::new(out.into_inner())
    }

    fn file(name: &str, bytes: Arc<Vec<u8>>) -> MarkFile {
        MarkFile { name: name.into(), bytes, page: 0 }
    }

    fn open(session: &mut Session, rotation: i64) -> crate::DocId {
        let blank = session.create_blank(300.0, 400.0, 1).unwrap();
        let id = session.open_new("form.pdf", blank).unwrap();
        if rotation != 0 {
            session.apply(id, Edit::RotatePages { pages: vec![0], degrees: rotation }).unwrap();
        }
        id
    }

    fn stamp_and_signature() -> Vec<GroupPicture> {
        vec![
            GroupPicture { name: "Company".into(), file: file("stamp.png", png(120, 60)), width: None, offset: [0.0, 0.0] },
            GroupPicture { name: "Signature".into(), file: file("signature.png", png(90, 30)), width: Some(60.0), offset: [0.0, 15.0] },
        ]
    }

    fn displayed(info: &PageInfo, rect: [f32; 4]) -> (f32, f32, f32, f32) {
        let a = info.user_to_view(rect[0], rect[1]);
        let b = info.user_to_view(rect[2], rect[3]);
        ((a[0] - b[0]).abs(), (a[1] - b[1]).abs(), (a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0)
    }

    #[test]
    fn picture_sizes_come_from_the_image_or_the_displayed_page() {
        assert_eq!(picture_size(&file("stamp.png", png(120, 60))).unwrap(), (120.0, 60.0));
        let mut session = Session::new();
        let turned = session.create_blank(300.0, 400.0, 1).unwrap();
        let id = session.open_new("turned.pdf", turned).unwrap();
        session.apply(id, Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
        let bytes = session.save_bytes(id).unwrap();
        assert_eq!(picture_size(&file("turned.pdf", bytes)).unwrap(), (400.0, 300.0));
        assert!(picture_size(&file("junk.png", Arc::new(b"not an image".to_vec()))).is_err());
        assert!(picture_size(&MarkFile { name: "x.pdf".into(), bytes: Arc::new(b"%PDF-1.7 junk".to_vec()), page: 3 }).is_err());
    }

    #[test]
    fn a_group_keeps_each_pictures_proportions_and_undoes_as_one_step() {
        let mut session = Session::new();
        let id = open(&mut session, 0);
        let info = session.get(id).unwrap().info.pages[0].clone();
        let edit = stamp_group(0, &info, [150.0, 200.0], &stamp_and_signature(), "Ada").unwrap();
        assert_eq!(edit.label().as_str(), STAMP_GROUP_LABEL);
        session.apply(id, edit).unwrap();
        let doc = session.get(id).unwrap();
        let stamps: Vec<_> = doc.info.annotations.iter().filter(|a| a.subtype == "Stamp").collect();
        assert_eq!(stamps.len(), 2);
        // The stamp at its natural size, centred on the anchor; the signature 60 pt wide, 15 pt lower.
        let (w, h, cx, cy) = displayed(&info, stamps[0].rect);
        assert!((w - 120.0).abs() < 0.01 && (h - 60.0).abs() < 0.01, "{w} x {h}");
        assert!((cx - 150.0).abs() < 0.01 && (cy - 200.0).abs() < 0.01, "{cx}, {cy}");
        let (w, h, cx, cy) = displayed(&info, stamps[1].rect);
        assert!((w - 60.0).abs() < 0.01 && (h - 20.0).abs() < 0.01, "{w} x {h}");
        assert!((cx - 150.0).abs() < 0.01 && (cy - 215.0).abs() < 0.01, "{cx}, {cy}");
        assert_eq!(stamps.iter().map(|s| s.contents.as_deref()).collect::<Vec<_>>(), [Some("Company"), Some("Signature")]);
        assert_eq!(doc.can_undo(), Some(STAMP_GROUP_LABEL));
        session.undo(id).unwrap();
        assert!(session.get(id).unwrap().info.annotations.is_empty(), "one undo takes the whole group back");
        session.redo(id).unwrap();
        assert_eq!(session.get(id).unwrap().info.annotations.len(), 2);
    }

    #[test]
    fn groups_read_upright_and_keep_their_displayed_layout_on_turned_pages() {
        for rotation in [90, 180, 270] {
            let mut session = Session::new();
            let id = open(&mut session, rotation);
            let info = session.get(id).unwrap().info.pages[0].clone();
            let edit = stamp_group(0, &info, [100.0, 120.0], &stamp_and_signature(), "Ada").unwrap();
            session.apply(id, edit).unwrap();
            let doc = session.get(id).unwrap();
            let stamps: Vec<_> = doc.info.annotations.iter().filter(|a| a.subtype == "Stamp").collect();
            assert_eq!(stamps.len(), 2, "rotation {rotation}");
            let (w, h, cx, cy) = displayed(&info, stamps[0].rect);
            assert!((w - 120.0).abs() < 0.01 && (h - 60.0).abs() < 0.01, "rotation {rotation}: {w} x {h}");
            assert!((cx - 100.0).abs() < 0.01 && (cy - 120.0).abs() < 0.01, "rotation {rotation}: {cx}, {cy}");
            let (w, h, _, cy) = displayed(&info, stamps[1].rect);
            assert!((w - 60.0).abs() < 0.01 && (h - 20.0).abs() < 0.01, "rotation {rotation}: {w} x {h}");
            assert!((cy - 135.0).abs() < 0.01, "rotation {rotation}: {cy}");
            // Both pictures are drawn turned back by the page's rotation, so they read upright.
            let cos = &doc.editor.as_ref().unwrap().cos;
            for s in &stamps {
                assert_eq!(pdfcraft_annot::picture_rotation(cos, 0, s.index).unwrap(), rotation, "rotation {rotation}");
            }
        }
    }

    #[test]
    fn bad_groups_are_refused_before_anything_changes() {
        let mut session = Session::new();
        let id = open(&mut session, 0);
        let info = session.get(id).unwrap().info.pages[0].clone();
        let one = |width: Option<f64>, offset: [f64; 2]| {
            vec![GroupPicture { name: "Company".into(), file: file("stamp.png", png(120, 60)), width, offset }]
        };
        assert!(stamp_group(0, &info, [10.0, 10.0], &[], "Ada").is_err());
        let many: Vec<_> = std::iter::repeat_n(one(None, [0.0, 0.0])[0].clone(), MAX_PICTURES + 1).collect();
        assert!(stamp_group(0, &info, [10.0, 10.0], &many, "Ada").is_err());
        for at in [[f64::NAN, 1.0], [1.0, f64::INFINITY], [1e9, 0.0]] {
            assert!(stamp_group(0, &info, at, &one(None, [0.0, 0.0]), "Ada").is_err(), "{at:?}");
        }
        for width in [0.0, -5.0, 0.5, 20_000.0, f64::NAN, f64::INFINITY] {
            assert!(stamp_group(0, &info, [10.0, 10.0], &one(Some(width), [0.0, 0.0]), "Ada").is_err(), "{width}");
        }
        assert!(stamp_group(0, &info, [10.0, 10.0], &one(None, [f64::NAN, 0.0]), "Ada").is_err());
        let junk = vec![GroupPicture { name: "Junk".into(), file: file("junk.png", Arc::new(vec![0; 64])), width: None, offset: [0.0, 0.0] }];
        assert!(stamp_group(0, &info, [10.0, 10.0], &junk, "Ada").is_err());
        let doc = session.get(id).unwrap();
        assert!(doc.info.annotations.is_empty() && doc.can_undo().is_none(), "a refusal changes nothing");
        // A whole group fails when one of its pictures does: nothing is placed.
        let mut mixed = one(None, [0.0, 0.0]);
        mixed.extend(junk);
        assert!(stamp_group(0, &info, [10.0, 10.0], &mixed, "Ada").is_err());
        assert!(session.get(id).unwrap().info.annotations.is_empty());
    }
}
