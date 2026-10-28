use crate::search::Tab;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabKey {
    pub window_id: i64,
    pub tab_id: i64,
}

impl From<&Tab> for TabKey {
    fn from(tab: &Tab) -> Self {
        Self {
            window_id: tab.window_id,
            tab_id: tab.tab_id,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub session: String,
    pub order: Vec<TabKey>,
    pub opened_tabs: Vec<TabKey>,
    pub opened_windows: Vec<i64>,
}

impl State {
    pub fn sync(&mut self, session: &str, tabs: &[Tab]) -> Vec<Tab> {
        if self.session != session {
            *self = Self {
                session: session.into(),
                ..Self::default()
            };
        }
        let live: Vec<TabKey> = tabs.iter().map(TabKey::from).collect();
        self.order = self
            .order
            .iter()
            .filter_map(|key| live.iter().find(|tab| tab.tab_id == key.tab_id).copied())
            .collect();
        for key in &live {
            if !self.order.contains(key) {
                self.order.push(*key);
            }
        }
        self.opened_tabs = self
            .opened_tabs
            .iter()
            .filter_map(|key| live.iter().find(|tab| tab.tab_id == key.tab_id).copied())
            .collect();
        self.opened_windows
            .retain(|id| tabs.iter().any(|tab| tab.window_id == *id));
        self.order
            .iter()
            .filter_map(|key| tabs.iter().find(|tab| TabKey::from(*tab) == *key).cloned())
            .collect()
    }
}

pub struct Store {
    directory: PathBuf,
    _lock: File,
    pub state: State,
}

impl Store {
    pub fn open() -> Result<Self, String> {
        let root = std::env::var_os("XDG_STATE_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            })
            .ok_or("HOME or XDG_STATE_HOME must be set")?;
        if !root.is_absolute() {
            return Err("state directory must be an absolute path".into());
        }
        Self::at(root.join("hel"))
    }

    fn at(directory: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&directory)
            .map_err(|e| format!("cannot create hel state directory: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let lock = private_file(&directory.join("lock"))?;
        lock.lock()
            .map_err(|e| format!("cannot lock hel state: {e}"))?;
        let state = match fs::read(directory.join("state.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
                "hel state is corrupt; move state.json aside to reset history".to_string()
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(e) => return Err(format!("cannot read hel state: {e}")),
        };
        Ok(Self {
            directory,
            _lock: lock,
            state,
        })
    }

    pub fn save(&self) -> Result<(), String> {
        use std::io::Write;
        let bytes = serde_json::to_vec(&self.state).map_err(|e| e.to_string())?;
        let temporary = self.directory.join("state.tmp");
        let mut file = private_file(&temporary)?;
        file.set_len(0).map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("cannot save hel state: {e}"))?;
        fs::rename(temporary, self.directory.join("state.json"))
            .map_err(|e| format!("cannot replace hel state: {e}"))
    }
}

fn private_file(path: &std::path::Path) -> Result<File, String> {
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err("hel state files must not be symbolic links".into());
    }
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|e| format!("cannot open hel state: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: i64) -> Tab {
        Tab {
            window_id: id,
            tab_id: id,
            title: "test".into(),
            url: "about:blank".into(),
        }
    }

    #[test]
    fn ordering_survives_focus_changes_and_prunes_closed_tabs() {
        let mut state = State::default();
        state.sync("a", &[tab(1), tab(2)]);
        state.opened_tabs = vec![TabKey::from(&tab(1)), TabKey::from(&tab(2))];
        state.opened_windows = vec![1, 2];
        assert_eq!(
            state.sync("a", &[tab(2), tab(1), tab(3)]),
            vec![tab(1), tab(2), tab(3)]
        );
        state.sync("a", &[tab(2), tab(3)]);
        assert_eq!(state.opened_tabs, vec![TabKey::from(&tab(2))]);
        assert_eq!(state.opened_windows, vec![2]);
        state.sync("b", &[tab(2)]);
        assert!(state.opened_tabs.is_empty());
        assert!(state.opened_windows.is_empty());
    }

    #[test]
    fn moving_tabs_preserves_numbers_and_creation_history() {
        let mut state = State::default();
        let original = tab(1);
        state.sync("a", &[original.clone(), tab(2)]);
        state.opened_tabs.push(TabKey::from(&original));
        let moved = Tab {
            window_id: 2,
            ..original
        };
        assert_eq!(
            state.sync("a", &[tab(2), moved.clone()]),
            vec![moved.clone(), tab(2)]
        );
        assert_eq!(state.opened_tabs, vec![TabKey::from(&moved)]);
    }

    #[test]
    fn state_round_trips_and_corruption_is_not_silently_discarded() {
        let dir = std::env::temp_dir().join(format!("hel-state-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        {
            let mut store = Store::at(dir.clone()).unwrap();
            store.state.session = "a".into();
            store.state.sync("a", &[tab(7)]);
            store.save().unwrap();
        }
        assert_eq!(
            Store::at(dir.clone()).unwrap().state.order,
            vec![TabKey::from(&tab(7))]
        );
        fs::write(dir.join("state.json"), b"broken").unwrap();
        assert!(Store::at(dir.clone()).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
