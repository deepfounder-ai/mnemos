//! Per-user on-disk directory layout.
//!
//! ```text
//! <data_dir>/
//!   mnemos.db
//!   users/<user_id>/
//!     pages/<slug>.md
//!     sources/<source_id>-<slug>.md
//!     index.md
//!     log.md
//! ```

use std::path::{Path, PathBuf};

use crate::error::Result;

/// Root directory that contains all user data.
pub fn data_root(data_dir: &Path) -> PathBuf {
    data_dir.to_path_buf()
}

/// Per-user directory, e.g. `data/users/<id>`.
pub fn user_dir(data_dir: &Path, user_id: &str) -> PathBuf {
    data_dir.join("users").join(user_id)
}

/// Directory holding the rendered wiki pages for a user.
pub fn pages_dir(data_dir: &Path, user_id: &str) -> PathBuf {
    user_dir(data_dir, user_id).join("pages")
}

/// Directory holding raw sources for a user.
pub fn sources_dir(data_dir: &Path, user_id: &str) -> PathBuf {
    user_dir(data_dir, user_id).join("sources")
}

/// Path to the rendered index.md.
pub fn index_path(data_dir: &Path, user_id: &str) -> PathBuf {
    user_dir(data_dir, user_id).join("index.md")
}

/// Path to the append-only log.md.
pub fn log_path(data_dir: &Path, user_id: &str) -> PathBuf {
    user_dir(data_dir, user_id).join("log.md")
}

/// Resolve the on-disk location for a source file.
pub fn source_path(data_dir: &Path, user_id: &str, source_id: &str, slug: &str) -> PathBuf {
    sources_dir(data_dir, user_id).join(format!("{source_id}-{slug}.md"))
}

/// Ensure all directories for a user exist.
pub fn ensure_user_dirs(data_dir: &Path, user_id: &str) -> Result<()> {
    let user = user_dir(data_dir, user_id);
    for d in [
        user.clone(),
        pages_dir(data_dir, user_id),
        sources_dir(data_dir, user_id),
    ] {
        std::fs::create_dir_all(&d)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_stable() {
        let root = Path::new("/tmp/mnemos-test");
        assert_eq!(user_dir(root, "u1"), PathBuf::from("/tmp/mnemos-test/users/u1"));
        assert_eq!(
            pages_dir(root, "u1"),
            PathBuf::from("/tmp/mnemos-test/users/u1/pages")
        );
        assert_eq!(
            source_path(root, "u1", "abc", "kafka"),
            PathBuf::from("/tmp/mnemos-test/users/u1/sources/abc-kafka.md")
        );
    }
}
