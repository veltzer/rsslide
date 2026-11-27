//! Helpers for the unit tests.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A file in a directory of its own under the system's temp directory.
/// The directory goes when the value is dropped, so a test leaves nothing
/// behind, whether it passes or panics.
pub struct TempFile {
    dir: PathBuf,
    path: PathBuf,
}

impl TempFile {
    /// `bytes` written to a new file called `name`.
    pub fn new(name: &str, bytes: &[u8]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rsslide-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("the temp directory is writable");
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("the temp directory is writable");
        Self { dir, path }
    }

    /// Where the file is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The path as text, for the places that take a file name in YAML.
    pub fn as_str(&self) -> &str {
        self.path.to_str().expect("the temp path is UTF-8")
    }
}

impl fmt::Debug for TempFile {
    /// The path as a quoted string, which is how a deck's YAML names it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // Nothing to do about a directory that will not go: the test's
        // own result is what matters.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
