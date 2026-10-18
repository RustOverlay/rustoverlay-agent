use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::Revision;

/// Which of the player's teams this install broadcasts to. One at a time, by choice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActiveTeam {
    pub team_id: String,
    pub join_code: String,
    pub name: String,
}

/// Plain JSON, unlike the install token: a join code is a shared credential the owner can rotate,
/// not a per-install secret, and the file sits in the user's own profile.
/// Clones share one [`Revision`], so the uplink's copy sees every save made through the UI's.
#[derive(Debug, Clone)]
pub struct ActiveTeamStore {
    directory: PathBuf,
    path: PathBuf,
    revision: Revision,
}

impl ActiveTeamStore {
    /// `file_name` comes from `BackendOptions::file_name`: a team id is only good at its backend.
    pub fn new(directory: &Path, file_name: &str) -> Self {
        Self {
            directory: directory.to_path_buf(),
            path: directory.join(file_name),
            revision: Revision::default(),
        }
    }

    /// Ticks on every successful save, including clearing the team.
    pub fn changes(&self) -> watch::Receiver<u64> {
        self.revision.watch()
    }

    /// None when no team is active, or the file is unreadable.
    pub fn load(&self) -> Option<ActiveTeam> {
        let bytes = fs::read(&self.path).ok()?;
        let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
        serde_json::from_slice::<Option<ActiveTeam>>(text).ok().flatten()
    }

    pub fn save(&self, team: Option<&ActiveTeam>) -> io::Result<()> {
        fs::create_dir_all(&self.directory)?;

        match team {
            Some(team) => crate::atomic_file::write(&self.path, &serde_json::to_vec(team)?)?,
            None => match fs::remove_file(&self.path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                other => other?,
            },
        }
        self.revision.bump();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_clears_the_active_team() {
        let dir = tempfile::tempdir().unwrap();
        let store = ActiveTeamStore::new(&dir.path().join("nested"), "active-team.json");
        let team = ActiveTeam { team_id: "t1".into(), join_code: "7926ZG".into(), name: "Squad".into() };

        let mut changes = store.clone().changes();

        assert_eq!(store.load(), None);
        store.save(Some(&team)).unwrap();
        assert!(changes.has_changed().unwrap());
        changes.mark_unchanged();
        assert_eq!(
            fs::read_to_string(dir.path().join("nested/active-team.json")).unwrap(),
            r#"{"team_id":"t1","join_code":"7926ZG","name":"Squad"}"#
        );
        assert_eq!(store.load(), Some(team));

        store.save(None).unwrap();
        assert_eq!(store.load(), None);
        assert!(changes.has_changed().unwrap());
        store.save(None).unwrap();
    }

    #[test]
    fn a_corrupt_file_reads_as_no_team() {
        let dir = tempfile::tempdir().unwrap();
        let store = ActiveTeamStore::new(dir.path(), "active-team.json");
        fs::write(dir.path().join("active-team.json"), "{not json").unwrap();

        assert_eq!(store.load(), None);
    }
}
