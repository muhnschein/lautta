// SPDX-License-Identifier: LGPL-2.1-or-later
//! Where the app keeps things (SPEC §3.3, DAT-1, PRV-6, EDT-1).

use std::path::{Path, PathBuf};

pub const ORGANIZATION: &str = "org.netvfs";
pub const APPLICATION: &str = "lautta";

/// The app's folders, derived from `$HOME`. Tests point `home` at a temp dir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub home: PathBuf,
}

impl AppPaths {
    pub fn new(home: impl Into<PathBuf>) -> AppPaths {
        AppPaths { home: home.into() }
    }

    pub fn from_env() -> AppPaths {
        AppPaths::new(
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/")),
        )
    }

    /// `~/.local/share/org.netvfs/lautta`
    pub fn data_dir(&self) -> PathBuf {
        self.home
            .join(".local/share")
            .join(ORGANIZATION)
            .join(APPLICATION)
    }

    /// `~/.cache/org.netvfs/lautta`
    pub fn cache_dir(&self) -> PathBuf {
        self.home.join(".cache").join(ORGANIZATION).join(APPLICATION)
    }

    pub fn database(&self) -> PathBuf {
        self.data_dir().join("lautta.db")
    }

    pub fn trash_dir(&self) -> PathBuf {
        self.data_dir().join("trash")
    }

    /// The bridge socket (NVB-1). Owned by netvfs; the app never creates it.
    pub fn bridge_socket(&self) -> PathBuf {
        self.data_dir().join("netvfs/bridge.sock")
    }

    pub fn thumbs_dir(&self) -> PathBuf {
        self.cache_dir().join("thumbs")
    }

    pub fn dircache_dir(&self) -> PathBuf {
        self.cache_dir().join("dircache")
    }

    /// Remote files opened in other apps (PRV-6).
    pub fn opened_dir(&self) -> PathBuf {
        self.home.join("Downloads/Lautta/Opened")
    }

    /// Working copies for edit-in-place (EDT-1).
    pub fn editing_dir(&self) -> PathBuf {
        self.home.join("Downloads/Lautta/Editing")
    }

    /// Removable media mount root (LOC-3).
    pub fn media_root(&self, user: &str) -> PathBuf {
        Path::new("/run/media").join(user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_spec() {
        let p = AppPaths::new("/home/defaultuser");
        assert_eq!(
            p.database(),
            Path::new("/home/defaultuser/.local/share/org.netvfs/lautta/lautta.db")
        );
        assert_eq!(
            p.bridge_socket(),
            Path::new("/home/defaultuser/.local/share/org.netvfs/lautta/netvfs/bridge.sock")
        );
        assert_eq!(
            p.trash_dir(),
            Path::new("/home/defaultuser/.local/share/org.netvfs/lautta/trash")
        );
        assert_eq!(
            p.thumbs_dir(),
            Path::new("/home/defaultuser/.cache/org.netvfs/lautta/thumbs")
        );
        assert_eq!(
            p.opened_dir(),
            Path::new("/home/defaultuser/Downloads/Lautta/Opened")
        );
        assert_eq!(
            p.editing_dir(),
            Path::new("/home/defaultuser/Downloads/Lautta/Editing")
        );
        assert_eq!(p.media_root("defaultuser"), Path::new("/run/media/defaultuser"));
    }
}
