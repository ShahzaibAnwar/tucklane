//! Test helpers. Every test works inside a throwaway sandbox under
//! `target/test-sandbox`, never in a real user folder.

use std::path::{Path, PathBuf};

use parking_lot::Mutex;

use crate::database::Db;
use crate::security::{AuthorizedRootGuard, RootPolicy};

pub fn sandbox() -> tempfile::TempDir {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-sandbox");
    std::fs::create_dir_all(&base).unwrap();
    tempfile::Builder::new()
        .prefix("tl-")
        .tempdir_in(base)
        .unwrap()
}

/// A sandbox containing a `Downloads` root plus sibling "outside" folders.
pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub root: PathBuf,
    pub outside: PathBuf,
    pub guard: AuthorizedRootGuard,
    pub db: Mutex<Db>,
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}

impl Fixture {
    pub fn new() -> Fixture {
        let dir = sandbox();
        let root = dir.path().join("Downloads");
        let outside = dir.path().join("Desktop");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let guard = AuthorizedRootGuard::open(&root, "root-1", &RootPolicy::system(&[])).unwrap();
        let db = Db::open_in_memory().unwrap();
        Fixture {
            dir,
            root,
            outside,
            guard,
            db: Mutex::new(db),
        }
    }

    /// Writes a file with an old modification time so it counts as settled.
    pub fn file(&self, rel: &str, contents: &[u8]) -> PathBuf {
        let p = self.root.join(rel.replace('/', "\\"));
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, contents).unwrap();
        age(&p);
        p
    }
}

/// Sets mtime one hour in the past.
pub fn age(p: &Path) {
    let f = std::fs::OpenOptions::new().write(true).open(p).unwrap();
    f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
        .unwrap();
}
