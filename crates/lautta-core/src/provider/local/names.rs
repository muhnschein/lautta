// SPDX-License-Identifier: LGPL-2.1-or-later
//! uid/gid to name resolution by parsing `/etc/passwd` and `/etc/group`
//! (BRW-1 owner/group). No NSS: the sandbox only sees the plain files, and
//! unknown ids fall back to their number.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Default)]
struct Tables {
    users: HashMap<u32, String>,
    groups: HashMap<u32, String>,
}

/// Lazily loaded, cached id to name tables.
pub struct Names {
    passwd: PathBuf,
    group: PathBuf,
    tables: OnceLock<Tables>,
}

impl Default for Names {
    fn default() -> Names {
        Names::from_files("/etc/passwd", "/etc/group")
    }
}

impl Names {
    pub fn from_files(passwd: impl Into<PathBuf>, group: impl Into<PathBuf>) -> Names {
        Names {
            passwd: passwd.into(),
            group: group.into(),
            tables: OnceLock::new(),
        }
    }

    fn tables(&self) -> &Tables {
        self.tables.get_or_init(|| Tables {
            users: parse_ids(&self.passwd),
            groups: parse_ids(&self.group),
        })
    }

    pub fn user(&self, uid: u32) -> String {
        self.tables()
            .users
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| uid.to_string())
    }

    pub fn group(&self, gid: u32) -> String {
        self.tables()
            .groups
            .get(&gid)
            .cloned()
            .unwrap_or_else(|| gid.to_string())
    }
}

/// Both files are `name:x:id:...`; the first entry for an id wins.
fn parse_ids(path: &Path) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return map;
    };
    for line in text.lines() {
        let mut fields = line.split(':');
        let (Some(name), Some(_), Some(id)) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        if name.is_empty() || name.starts_with('#') {
            continue;
        }
        if let Ok(id) = id.parse::<u32>() {
            map.entry(id).or_insert_with(|| name.to_owned());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_known_and_falls_back_to_numbers() {
        let d = tempfile::tempdir().unwrap();
        let (p, g) = (d.path().join("passwd"), d.path().join("group"));
        std::fs::write(
            &p,
            "root:x:0:0:root:/root:/bin/sh\n#comment:x:5:5\nbroken line\n\
             defaultuser:x:100000:100000::/home/defaultuser:/bin/sh\ndup:x:0:0\n",
        )
        .unwrap();
        std::fs::write(&g, "root:x:0:\nprivileged:x:996:defaultuser\n:x:7:\n").unwrap();
        let n = Names::from_files(&p, &g);
        assert_eq!(n.user(0), "root");
        assert_eq!(n.user(100_000), "defaultuser");
        assert_eq!(n.user(5), "5");
        assert_eq!(n.user(424_242), "424242");
        assert_eq!(n.group(996), "privileged");
        assert_eq!(n.group(7), "7");
        assert_eq!(n.group(31337), "31337");
    }

    #[test]
    fn missing_files_give_numbers() {
        let n = Names::from_files("/nonexistent/passwd", "/nonexistent/group");
        assert_eq!(n.user(1000), "1000");
        assert_eq!(n.group(1000), "1000");
    }

    #[test]
    fn tables_are_cached_after_first_use() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("passwd");
        std::fs::write(&p, "a:x:1:1\n").unwrap();
        let n = Names::from_files(&p, d.path().join("group"));
        assert_eq!(n.user(1), "a");
        std::fs::write(&p, "b:x:1:1\n").unwrap();
        assert_eq!(n.user(1), "a");
    }
}
