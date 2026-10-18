#![cfg(windows)]

use std::path::PathBuf;
use std::sync::Arc;

use ro_capture::{bitmap_frames, frame_image, icon_signatures, FontGlyphAtlas, SignatureCache, StopToken, Stopped};
use ro_vision::capture::ScreenRect;
use ro_vision::hud::{ArrayFrameBuffer, FrameBuffer, GlyphAtlas, Rgb};
use ro_vision::items::{IconFace, IconSignature, ItemDefinition};

fn item(short_name: &str, icon: Option<PathBuf>) -> IconFace {
    IconFace::from(Arc::new(ItemDefinition {
        item_id: 1,
        short_name: short_name.into(),
        name: short_name.into(),
        category: "Weapon".into(),
        is_wearable: false,
        is_holdable: true,
        has_condition_bar: false,
        stack_size: 1,
        icon_path: icon,
    }))
}

/// A BGRA frame with a distinct colour in every pixel and a padded stride.
fn gradient(width: i32, height: i32) -> ArrayFrameBuffer {
    let stride = width * 4 + 12;
    let mut pixels = vec![0u8; (stride * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let i = (y * stride + x * 4) as usize;
            pixels[i..i + 4].copy_from_slice(&[(x * 7) as u8, (y * 11) as u8, (x + y) as u8, 0]);
        }
    }
    ArrayFrameBuffer::new(pixels, stride, ScreenRect::new(0, 0, width, height))
}

#[test]
fn every_count_character_is_rendered_at_the_asked_height_in_order() {
    let atlas = FontGlyphAtlas::new().unwrap();

    for height in [9, 12, 17] {
        let glyphs = atlas.at(height);
        let order: String = glyphs.iter().map(|(c, _)| c).collect();

        assert_eq!(order, "0123456789x");
        for (ch, glyph) in glyphs.iter() {
            assert_eq!(glyph.height, height, "{ch}");
            assert!(glyph.width >= 1, "{ch}");
            assert_eq!(glyph.cell.len(), (glyph.width * glyph.height) as usize, "{ch}");
            assert!(glyph.cell.iter().any(|&v| v > 128.0), "{ch} has no ink");
        }
    }
}

#[test]
fn x_sits_below_the_cap_line_rather_than_stretched_to_it() {
    let atlas = FontGlyphAtlas::new().unwrap();
    let x = atlas.at(16).get('x').unwrap().clone();

    let top_row_ink = x.cell[..x.width as usize].iter().fold(0.0f64, |a, &b| a.max(b));
    assert!(top_row_ink < 64.0, "x reached the cap line: {top_row_ink}");
}

#[test]
fn each_height_is_rendered_once() {
    let atlas = FontGlyphAtlas::new().unwrap();

    assert!(Arc::ptr_eq(&atlas.at(14), &atlas.at(14)));
}

#[test]
fn a_saved_frame_reads_back_pixel_for_pixel() {
    let dir = tempfile::tempdir().unwrap();
    let frame = gradient(37, 23);

    let path = frame_image::save(&frame, &dir.path().join(frame_image::DIRECTORY_NAME)).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("frame-") && name.ends_with("-37x23.png"), "{name}");

    let read = bitmap_frames::read_file(&path).unwrap();
    assert_eq!((read.width(), read.height()), (37, 23));
    for y in 0..23 {
        for x in 0..37 {
            assert_eq!(read.pixel(x, y), frame.pixel(x, y), "at {x},{y}");
        }
    }
}

#[test]
fn an_opaque_flat_icon_signs_as_its_own_colour() {
    let dir = tempfile::tempdir().unwrap();
    let mut pixels = Vec::new();
    for _ in 0..64 * 64 {
        pixels.extend_from_slice(&[0x40, 0x80, 0xC0, 0xFF]);
    }
    let flat = ArrayFrameBuffer::new(pixels, 64 * 4, ScreenRect::new(0, 0, 64, 64));
    assert_eq!(flat.pixel(5, 5), Rgb::new(0xC0, 0x80, 0x40));
    let path = frame_image::save(&flat, dir.path()).unwrap();

    let signature = icon_signatures::from_file(&path).unwrap();

    // Interior only: the bicubic kernel under SourceCopy fades alpha along the border, as it did in C#.
    let n = IconSignature::SIZE;
    for y in 2..n - 2 {
        for x in 2..n - 2 {
            let cell = y * n + x;
            assert_eq!(signature.alpha()[cell], 1.0, "alpha at {x},{y}");
            assert_eq!(&signature.rgb()[cell * 3..cell * 3 + 3], &[192.0 / 255.0, 128.0 / 255.0, 64.0 / 255.0]);
        }
    }
}

#[test]
fn icons_that_will_not_decode_are_reported_not_raised() {
    let dir = tempfile::tempdir().unwrap();
    let good = frame_image::save(&gradient(40, 40), dir.path()).unwrap();
    let bytes = std::fs::read(&good).unwrap();
    let truncated = dir.path().join("truncated.png");
    // Cut inside the header: GDI+ decodes a PNG missing only trailing data without complaint.
    std::fs::write(&truncated, &bytes[..24]).unwrap();

    let faces = vec![
        item("missing", Some(dir.path().join("nope.png"))),
        item("good", Some(good)),
        item("no-icon", None),
        item("truncated", Some(truncated)),
    ];

    let (built, unreadable) = icon_signatures::build(faces);

    let built: Vec<&str> = built.keys().map(|f| f.item.short_name.as_str()).collect();
    let unreadable: Vec<&str> = unreadable.iter().map(|f| f.item.short_name.as_str()).collect();
    assert_eq!(built, ["good"]);
    assert_eq!(unreadable, ["missing", "truncated"]);
}

#[test]
fn a_build_stopped_before_it_starts_decodes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let icon = frame_image::save(&gradient(40, 40), dir.path()).unwrap();
    let cache_file = dir.path().join("cache.bin");
    let cache = SignatureCache::open(&cache_file);
    let stop = StopToken::new();
    stop.stop();

    let built = icon_signatures::build_cached_until(vec![item("good", Some(icon))], &cache, &stop);

    assert!(matches!(built, Err(Stopped)));
    cache.save().unwrap();
    assert!(!cache_file.exists(), "an icon was decoded into the cache");
}

#[test]
fn an_unstopped_build_until_matches_a_plain_build_and_fills_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    let icon = frame_image::save(&gradient(40, 40), dir.path()).unwrap();
    let faces = vec![item("good", Some(icon)), item("missing", Some(dir.path().join("nope.png")))];
    let cache_file = dir.path().join("cache.bin");
    let cache = SignatureCache::open(&cache_file);

    let (built, unreadable) = icon_signatures::build_cached_until(faces.clone(), &cache, &StopToken::new()).unwrap();
    let (plain, plain_unreadable) = icon_signatures::build(faces);

    assert_eq!(built.len(), 1);
    for (face, signature) in &plain {
        assert_eq!(built[face].rgb(), signature.rgb());
        assert_eq!(built[face].alpha(), signature.alpha());
    }
    assert_eq!(unreadable.len(), plain_unreadable.len());
    cache.save().unwrap();
    assert!(cache_file.exists());
}
