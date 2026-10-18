//! Checks against the installed game and the UnityPy reference output; each test skips when its
//! inputs are absent. Env: RO_UNITY_TEXTURES_DIR, RO_UNITY_PARITY_JSON, RO_UNITY_REF_PNG_DIR.

use std::path::PathBuf;

use super::*;

const DEFAULT_TEXTURES: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\Rust\Bundles\textures";

fn textures_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("RO_UNITY_TEXTURES_DIR")
        .map_or_else(|| PathBuf::from(DEFAULT_TEXTURES), PathBuf::from);
    dir.is_dir().then_some(dir)
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.exists())
}

/// `{wid: [bundle_index, abs_offset, size, width, height, format]}` from `index_json.py`.
#[test]
fn index_matches_unitypy() {
    let (Some(dir), Some(json)) = (textures_dir(), env_path("RO_UNITY_PARITY_JSON")) else {
        eprintln!("skipped: game textures or RO_UNITY_PARITY_JSON missing");
        return;
    };
    let reference: HashMap<String, Vec<u64>> =
        serde_json::from_slice(&std::fs::read(json).unwrap()).unwrap();
    let index = SkinIconIndex::build(&dir).unwrap();
    assert_eq!(index.len(), reference.len(), "icon count");
    let mut matched = 0;
    for (wid, want) in &reference {
        let loc = &index.icons[&wid.parse::<u64>().unwrap()];
        let bundle_name = format!("textures.{}.bundle", want[0]);
        assert_eq!(
            index.bundles[loc.bundle]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
            bundle_name,
            "{wid}"
        );
        let got = [
            loc.offset,
            u64::from(loc.size),
            u64::from(loc.width),
            u64::from(loc.height),
            loc.format as u64,
        ];
        assert_eq!(got.as_slice(), &want[1..], "{wid}");
        matched += 1;
    }
    eprintln!("matched {matched} of {} icons", reference.len());
}

fn load_png(path: &Path) -> (u32, u32, Vec<u8>) {
    let mut decoder = png::Decoder::new(std::io::BufReader::new(File::open(path).unwrap()));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba, "{}", path.display());
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

#[test]
fn decoded_icons_match_unitypy_pngs() {
    let (Some(dir), Some(pngs)) = (textures_dir(), env_path("RO_UNITY_REF_PNG_DIR")) else {
        eprintln!("skipped: game textures or RO_UNITY_REF_PNG_DIR missing");
        return;
    };
    let index = SkinIconIndex::build(&dir).unwrap();
    for wid in [3208884292u64, 3137160380] {
        let img = index.read_icon(wid).unwrap().expect("icon indexed");
        let (w, h, want) = load_png(&pngs.join(format!("{wid}.png")));
        assert_eq!((img.width, img.height), (w, h));
        let got = img.pixels.iter().flatten();
        let diffs: Vec<u8> = got.zip(&want).map(|(a, b)| a.abs_diff(*b)).collect();
        let max = diffs.iter().copied().max().unwrap();
        let differing = diffs.iter().filter(|&&d| d != 0).count();
        eprintln!(
            "{wid}: {w}x{h}, max channel diff {max}, {differing} of {} channels differ",
            diffs.len()
        );
        assert_eq!(max, 0, "{wid}: decoder rounding diverged from Pillow");
    }
    assert_eq!(index.read_icon(1).unwrap(), None);
}
