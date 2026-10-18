use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Writes beside `path` and renames over it, so a crash mid-write leaves the old file intact.
/// On Windows `fs::rename` replaces an existing file (MoveFileEx with REPLACE_EXISTING).
pub(crate) fn write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let temp = temp_path(path);
    let written = File::create(&temp).and_then(|mut file| {
        file.write_all(contents)?;
        file.sync_all()
    });

    match written.and_then(|()| fs::rename(&temp, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&temp);
            Err(e)
        }
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_the_file_and_leaves_no_temp_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, b"old contents").unwrap();

        write(&path, b"new").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert!(!dir.path().join("state.json.tmp").exists());
    }

    #[test]
    fn a_failed_write_keeps_the_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, b"old").unwrap();
        // A directory where the temp file should go makes the create fail.
        fs::create_dir(dir.path().join("state.json.tmp")).unwrap();

        assert!(write(&path, b"new").is_err());

        assert_eq!(fs::read(&path).unwrap(), b"old");
    }
}
