//! On-disk cache of the 32x32 GDI+ downscales behind every [`IconSignature`].
//!
//! Decoding ~1250 catalogue PNGs at 512x512 costs seconds of CPU on every launch, yet the result is
//! a pure function of the file. The cache stores the exact BGRA bytes GDI+ produced, so a hit
//! rebuilds a bit-identical signature. Keyed on path, length and modification time; bump
//! [`VERSION`] whenever `icon_signatures` changes how it renders.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::UNIX_EPOCH;

const MAGIC: &[u8; 4] = b"ROSC";
const VERSION: u32 = 1;
pub(crate) const DOWNSCALE_BYTES: usize = ro_vision::items::IconSignature::SIZE * ro_vision::items::IconSignature::SIZE * 4;

/// What identifies one revision of an icon file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Stamp {
    len: u64,
    modified_nanos: u128,
}

impl Stamp {
    pub(crate) fn of(path: &Path) -> Option<Self> {
        let meta = fs::metadata(path).ok()?;
        let modified_nanos = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
        Some(Self { len: meta.len(), modified_nanos })
    }
}

type Map = HashMap<PathBuf, (Stamp, Box<[u8]>)>;

#[derive(Default)]
struct Entries {
    map: Map,
    /// Paths hit or stored this session; see [`SignatureCache::retain_touched`].
    touched: HashSet<PathBuf>,
    dirty: bool,
}

pub struct SignatureCache {
    file: PathBuf,
    entries: Mutex<Entries>,
}

impl SignatureCache {
    /// Loads `file`; a missing, truncated or foreign file is an empty cache, never an error.
    pub fn open(file: impl Into<PathBuf>) -> Self {
        let file = file.into();
        let map = fs::read(&file).ok().and_then(|bytes| decode(&bytes)).unwrap_or_default();
        Self { file, entries: Mutex::new(Entries { map, ..Entries::default() }) }
    }

    pub(crate) fn get(&self, path: &Path, stamp: Stamp) -> Option<Box<[u8]>> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let bytes = entries.map.get(path).filter(|(s, _)| *s == stamp).map(|(_, bytes)| bytes.clone())?;
        entries.touched.insert(path.to_owned());
        Some(bytes)
    }

    pub(crate) fn put(&self, path: PathBuf, stamp: Stamp, bytes: Box<[u8]>) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.touched.insert(path.clone());
        entries.map.insert(path, (stamp, bytes));
        entries.dirty = true;
    }

    /// Drops every entry not read or stored this session, so icons Steam evicted or a moved library
    /// stop being carried forever. Only sound once every live icon has been looked up.
    pub(crate) fn retain_touched(&self) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let Entries { map, touched, dirty } = &mut *entries;
        let before = map.len();
        map.retain(|path, _| touched.contains(path));
        *dirty |= map.len() != before;
    }

    /// Writes the cache if anything changed, through a temporary file so a crash mid-write leaves
    /// the previous cache intact.
    pub fn save(&self) -> io::Result<()> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if !entries.dirty {
            return Ok(());
        }

        if let Some(parent) = self.file.parent() {
            fs::create_dir_all(parent)?;
        }
        // Per process, so two writers sharing the cache cannot interleave into one temporary.
        let temporary = self.file.with_extension(format!("{}.tmp", std::process::id()));
        let mut file = fs::File::create(&temporary)?;
        file.write_all(&encode(&entries.map))?;
        // Gotcha: without the sync NTFS can persist the rename before the data after a power cut.
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &self.file)?;
        entries.dirty = false;
        Ok(())
    }
}

fn encode(map: &Map) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + map.len() * (DOWNSCALE_BYTES + 128));
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(map.len() as u64).to_le_bytes());

    for (path, (stamp, bytes)) in map {
        let path = path.to_string_lossy();
        out.extend_from_slice(&(path.len() as u32).to_le_bytes());
        out.extend_from_slice(path.as_bytes());
        out.extend_from_slice(&stamp.len.to_le_bytes());
        out.extend_from_slice(&stamp.modified_nanos.to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

fn decode(bytes: &[u8]) -> Option<Map> {
    let mut input = bytes;
    let mut magic = [0u8; 4];
    input.read_exact(&mut magic).ok()?;
    if &magic != MAGIC || u32::from_le_bytes(take(&mut input)?) != VERSION {
        return None;
    }

    let count = u64::from_le_bytes(take(&mut input)?);
    let mut map = HashMap::new();
    for _ in 0..count {
        let path_len = u32::from_le_bytes(take(&mut input)?) as usize;
        let path = std::str::from_utf8(input.get(..path_len)?).ok()?;
        input = &input[path_len..];
        let stamp = Stamp { len: u64::from_le_bytes(take(&mut input)?), modified_nanos: u128::from_le_bytes(take(&mut input)?) };
        let pixels = input.get(..DOWNSCALE_BYTES)?.into();
        input = &input[DOWNSCALE_BYTES..];
        map.insert(PathBuf::from(path), (stamp, pixels));
    }

    input.is_empty().then_some(map)
}

fn take<const N: usize>(input: &mut &[u8]) -> Option<[u8; N]> {
    let mut buffer = [0u8; N];
    input.read_exact(&mut buffer).ok()?;
    Some(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(len: u64) -> Stamp {
        Stamp { len, modified_nanos: 1_700_000_000_123_456_789 }
    }

    fn pixels() -> Box<[u8]> {
        (0..DOWNSCALE_BYTES).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn entries_round_trip_through_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("cache.bin");

        let cache = SignatureCache::open(&file);
        cache.put(PathBuf::from(r"C:\icons\rifle.ak.png"), stamp(42), pixels());
        cache.save().unwrap();

        let reopened = SignatureCache::open(&file);
        assert_eq!(reopened.get(Path::new(r"C:\icons\rifle.ak.png"), stamp(42)), Some(pixels()));
        assert_eq!(reopened.get(Path::new(r"C:\icons\rifle.ak.png"), stamp(43)), None);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1, "the temporary file is left behind");
    }

    #[test]
    fn only_entries_touched_this_session_survive_a_prune() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("cache.bin");

        let first = SignatureCache::open(&file);
        first.put(PathBuf::from("live.png"), stamp(1), pixels());
        first.put(PathBuf::from("evicted.png"), stamp(2), pixels());
        first.save().unwrap();

        let second = SignatureCache::open(&file);
        assert!(second.get(Path::new("live.png"), stamp(1)).is_some());
        second.put(PathBuf::from("new.png"), stamp(3), pixels());
        second.retain_touched();
        second.save().unwrap();

        let third = SignatureCache::open(&file);
        assert!(third.get(Path::new("live.png"), stamp(1)).is_some());
        assert!(third.get(Path::new("new.png"), stamp(3)).is_some());
        assert!(third.get(Path::new("evicted.png"), stamp(2)).is_none());
    }

    #[test]
    fn a_prune_that_drops_nothing_does_not_rewrite() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("cache.bin");

        let cache = SignatureCache::open(&file);
        cache.retain_touched();
        cache.save().unwrap();

        assert!(!file.exists());
    }

    #[test]
    fn a_damaged_file_is_an_empty_cache() {
        let mut bytes = encode(&HashMap::from([(PathBuf::from("a.png"), (stamp(1), vec![7u8; DOWNSCALE_BYTES].into()))]));
        assert!(decode(&bytes).is_some());

        bytes.truncate(bytes.len() - 1);
        assert!(decode(&bytes).is_none());
        assert!(decode(b"ROSC\x02\0\0\0").is_none());
    }
}
