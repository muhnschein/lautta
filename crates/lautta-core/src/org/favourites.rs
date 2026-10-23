// SPDX-License-Identifier: LGPL-2.1-or-later
//! Favourites (ORG-1): any folder or file, local or remote, with label,
//! colour and icon, reorderable. Whether it is a folder is stored with it, so
//! Browse can show and open a favourite without asking its location.

use super::{location_prefix, move_to, next_position, parse_uri, renumber, rewrite_uris};
use crate::db::Db;
use crate::error::{Error, ErrorKind, Result};
use crate::mime::{category_of_name, FileCategory};
use crate::uri::Uri;
use rusqlite::{params, Connection, OptionalExtension, Row};

/// The icons a favourite can have (ORG-1), by theme name
/// (`image://theme/icon-m-<name>`, all in the Sailfish OS 5.2 theme), in the
/// order the favourite page offers them. The first, the star, is what a
/// folder favourite shows until another is chosen.
pub const ICONS: [&str; 24] = [
    "favorite",
    "home",
    "company",
    "contact",
    "users",
    "note",
    "file-image",
    "camera",
    "file-video",
    "file-audio",
    "downloads",
    "file-folder",
    "cloud-download",
    "computer",
    "game-controller",
    "date",
    "mail",
    "website",
    "airplane-mode",
    "car",
    "health",
    "toy",
    "keys",
    "android",
];

/// `icon` when it is one of [`ICONS`]. Anything else (a name a later
/// version stored, a typo) is no choice: the favourite keeps its default
/// icon rather than showing an empty square.
fn known_icon(icon: Option<&str>) -> Option<&str> {
    icon.filter(|i| ICONS.contains(i))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Favourite {
    pub id: i64,
    pub uri: Uri,
    pub label: String,
    pub colour: Option<String>,
    pub position: i64,
    /// A folder; false for a file.
    pub is_dir: bool,
    /// The chosen icon, one of [`ICONS`]; `None` for the default one.
    pub icon: Option<String>,
}

impl Favourite {
    /// The theme icon Browse shows for the favourite: the chosen one, or
    /// the default ([`Favourite::default_icon`]).
    pub fn icon_source(&self) -> String {
        match known_icon(self.icon.as_deref()) {
            Some(icon) => format!("image://theme/icon-m-{icon}"),
            None => self.default_icon().to_owned(),
        }
    }

    /// The icon of a favourite without a chosen one: the star for a folder,
    /// the file's category for a file, as the folder view shows it.
    pub fn default_icon(&self) -> &'static str {
        if self.is_dir {
            return "image://theme/icon-m-favorite";
        }
        match category_of_name(self.uri.name().unwrap_or_default()) {
            FileCategory::Image => "image://theme/icon-m-file-image",
            FileCategory::Video => "image://theme/icon-m-file-video",
            FileCategory::Audio => "image://theme/icon-m-file-audio",
            FileCategory::Archive => "image://theme/icon-m-file-compressed",
            FileCategory::Package => "image://theme/icon-m-file-rpm",
            _ => "image://theme/icon-m-document",
        }
    }
}

#[derive(Clone)]
pub struct Favourites {
    db: Db,
}

type RawRow = (i64, String, String, Option<String>, i64, bool, Option<String>);

fn from_row(r: &Row<'_>) -> rusqlite::Result<RawRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
    ))
}

fn build(raw: RawRow) -> Result<Favourite> {
    Ok(Favourite {
        id: raw.0,
        uri: parse_uri(&raw.1)?,
        label: raw.2,
        colour: raw.3,
        position: raw.4,
        is_dir: raw.5,
        icon: known_icon(raw.6.as_deref()).map(str::to_owned),
    })
}

const COLUMNS: &str = "id, uri, label, colour, position, is_dir, icon";

fn clean_label(label: &str) -> Result<&str> {
    let label = label.trim();
    if label.is_empty() {
        return Err(Error::new(ErrorKind::InvalidArgument, "empty label"));
    }
    Ok(label)
}

/// Appends a row; returns its id.
fn insert(
    conn: &Connection,
    uri: &str,
    label: &str,
    colour: Option<&str>,
    icon: Option<&str>,
    is_dir: bool,
) -> Result<i64> {
    let position = next_position(conn, "favourites")?;
    conn.execute(
        "INSERT INTO favourites(uri, label, colour, position, is_dir, icon) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![uri, label, colour, position, is_dir, icon],
    )?;
    Ok(conn.last_insert_rowid())
}

/// The favourite stored as row `id`.
fn read(conn: &Connection, id: i64) -> Result<Favourite> {
    let raw = conn.query_row(
        &format!("SELECT {COLUMNS} FROM favourites WHERE id = ?1"),
        [id],
        from_row,
    )?;
    build(raw)
}

impl Favourites {
    pub fn new(db: Db) -> Favourites {
        Favourites { db }
    }

    /// Appends a favourite for the folder (`is_dir`) or file `uri`, with the
    /// default icon. An item can be a favourite once (`AlreadyExists`); an
    /// empty label is `InvalidArgument`.
    pub fn add(&self, uri: &Uri, label: &str, colour: Option<&str>, is_dir: bool) -> Result<Favourite> {
        let label = clean_label(label)?;
        let conn = self.db.lock();
        let key = uri.to_string();
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM favourites WHERE uri = ?1", [&key], |r| r.get(0))
            .optional()?;
        if exists.is_some() {
            return Err(Error::kind(ErrorKind::AlreadyExists));
        }
        let id = insert(&conn, &key, label, colour, None, is_dir)?;
        read(&conn, id)
    }

    /// Adds a favourite for the folder (`is_dir`) or file `uri`, or changes
    /// label, colour and icon of the one it already is, which keeps what it
    /// was stored as: what the favourite page does on every change, without
    /// the caller tracking ids. An icon that is not one of [`ICONS`] is
    /// stored as no choice.
    pub fn save(
        &self,
        uri: &Uri,
        label: &str,
        colour: Option<&str>,
        icon: Option<&str>,
        is_dir: bool,
    ) -> Result<Favourite> {
        let label = clean_label(label)?;
        let icon = known_icon(icon);
        let conn = self.db.lock();
        let key = uri.to_string();
        let existing: Option<i64> = conn
            .query_row("SELECT id FROM favourites WHERE uri = ?1", [&key], |r| r.get(0))
            .optional()?;
        let id = match existing {
            Some(id) => {
                conn.execute(
                    "UPDATE favourites SET label = ?1, colour = ?2, icon = ?3 WHERE id = ?4",
                    params![label, colour, icon, id],
                )?;
                id
            }
            None => insert(&conn, &key, label, colour, icon, is_dir)?,
        };
        read(&conn, id)
    }

    pub fn list(&self) -> Result<Vec<Favourite>> {
        let conn = self.db.lock();
        let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM favourites ORDER BY position, id"))?;
        let raw = stmt
            .query_map([], from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter().map(build).collect()
    }

    pub fn get_by_uri(&self, uri: &Uri) -> Result<Option<Favourite>> {
        let conn = self.db.lock();
        let raw = conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM favourites WHERE uri = ?1"),
                [uri.to_string()],
                from_row,
            )
            .optional()?;
        raw.map(build).transpose()
    }

    pub fn is_favourite(&self, uri: &Uri) -> Result<bool> {
        Ok(self.get_by_uri(uri)?.is_some())
    }

    pub fn update(&self, id: i64, label: &str, colour: Option<&str>) -> Result<()> {
        let label = clean_label(label)?;
        let n = self.db.lock().execute(
            "UPDATE favourites SET label = ?1, colour = ?2 WHERE id = ?3",
            params![label, colour, id],
        )?;
        if n == 0 {
            return Err(Error::kind(ErrorKind::NotFound));
        }
        Ok(())
    }

    /// Removes by id; returns whether a row existed.
    pub fn remove(&self, id: i64) -> Result<bool> {
        let conn = self.db.lock();
        let n = conn.execute("DELETE FROM favourites WHERE id = ?1", [id])?;
        renumber(&conn, "favourites")?;
        Ok(n > 0)
    }

    pub fn remove_uri(&self, uri: &Uri) -> Result<bool> {
        let conn = self.db.lock();
        let n = conn.execute("DELETE FROM favourites WHERE uri = ?1", [uri.to_string()])?;
        renumber(&conn, "favourites")?;
        Ok(n > 0)
    }

    /// Moves a favourite to `index` (clamped to the end).
    pub fn move_to(&self, id: i64, index: usize) -> Result<()> {
        move_to(&self.db.lock(), "favourites", id, index)
    }

    /// Follows an item moved or renamed by the app, including favourites
    /// inside it.
    pub fn on_moved(&self, old: &Uri, new: &Uri) -> Result<usize> {
        rewrite_uris(&self.db.lock(), "favourites", old, new)
    }

    /// SEC-5: forget every favourite of a removed location.
    pub fn remove_location(&self, location: &str) -> Result<usize> {
        let prefix = location_prefix(location);
        let conn = self.db.lock();
        let n = conn.execute(
            "DELETE FROM favourites WHERE substr(uri, 1, length(?1)) = ?1",
            [prefix],
        )?;
        renumber(&conn, "favourites")?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Uri {
        Uri::parse(s).unwrap()
    }

    fn store() -> Favourites {
        Favourites::new(Db::open_in_memory().unwrap())
    }

    #[test]
    fn add_list_in_order_with_positions() {
        let f = store();
        let a = f.add(&u("lautta://nv-1/a"), "  A ", Some("#f00"), true).unwrap();
        let b = f.add(&u("lautta://nv-1/b"), "B", None, true).unwrap();
        assert_eq!((a.position, b.position), (0, 1));
        assert_eq!(a.label, "A");
        let list = f.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].uri, u("lautta://nv-1/a"));
        assert_eq!(list[0].colour.as_deref(), Some("#f00"));
        assert_eq!(list[1].colour, None);
    }

    #[test]
    fn duplicates_and_empty_labels_are_refused() {
        let f = store();
        f.add(&u("lautta://nv-1/a"), "A", None, true).unwrap();
        assert_eq!(
            f.add(&u("lautta://nv-1/a"), "Again", None, true)
                .unwrap_err()
                .kind,
            ErrorKind::AlreadyExists
        );
        assert_eq!(
            f.add(&u("lautta://nv-1/b"), "   ", None, true).unwrap_err().kind,
            ErrorKind::InvalidArgument
        );
        assert_eq!(f.list().unwrap().len(), 1);
    }

    #[test]
    fn save_adds_new_folders_and_relabels_known_ones() {
        let f = store();
        let a = f.save(&u("lautta://nv-1/a"), "A", None, None, true).unwrap();
        let b = f
            .save(&u("lautta://nv-1/b"), " B ", Some("#0f0"), None, true)
            .unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!((a.position, b.position, b.label.as_str()), (0, 1, "B"));
        let again = f
            .save(&u("lautta://nv-1/a"), "Docs", Some("#f00"), None, true)
            .unwrap();
        assert_eq!((again.id, again.position), (a.id, 0));
        let list = f.list().unwrap();
        assert_eq!(
            list.iter()
                .map(|x| (x.label.as_str(), x.colour.as_deref()))
                .collect::<Vec<_>>(),
            [("Docs", Some("#f00")), ("B", Some("#0f0"))]
        );
        assert_eq!(
            f.save(&u("lautta://nv-1/c"), " ", None, None, true)
                .unwrap_err()
                .kind,
            ErrorKind::InvalidArgument
        );
        assert_eq!(f.list().unwrap().len(), 2);
    }

    #[test]
    fn update_and_lookup() {
        let f = store();
        let a = f.add(&u("lautta://nv-1/a"), "A", None, true).unwrap();
        f.update(a.id, "Docs", Some("#0f0")).unwrap();
        let got = f.get_by_uri(&u("lautta://nv-1/a")).unwrap().unwrap();
        assert_eq!(
            (got.label.as_str(), got.colour.as_deref()),
            ("Docs", Some("#0f0"))
        );
        assert!(f.is_favourite(&u("lautta://nv-1/a")).unwrap());
        assert!(!f.is_favourite(&u("lautta://nv-1/zzz")).unwrap());
        assert_eq!(f.update(999, "x", None).unwrap_err().kind, ErrorKind::NotFound);
        assert_eq!(
            f.update(a.id, " ", None).unwrap_err().kind,
            ErrorKind::InvalidArgument
        );
    }

    #[test]
    fn reorder_and_remove_keep_positions_dense() {
        let f = store();
        let ids: Vec<i64> = ["a", "b", "c"]
            .iter()
            .map(|n| f.add(&u(&format!("lautta://x/{n}")), n, None, true).unwrap().id)
            .collect();
        f.move_to(ids[2], 0).unwrap();
        let order: Vec<String> = f.list().unwrap().into_iter().map(|x| x.label).collect();
        assert_eq!(order, ["c", "a", "b"]);
        assert!(f.remove(ids[0]).unwrap());
        assert!(!f.remove(ids[0]).unwrap());
        let list = f.list().unwrap();
        assert_eq!(list.iter().map(|x| x.position).collect::<Vec<_>>(), [0, 1]);
        assert!(f.remove_uri(&u("lautta://x/b")).unwrap());
        assert_eq!(f.list().unwrap().len(), 1);
    }

    #[test]
    fn remove_location_only_touches_that_location() {
        let f = store();
        f.add(&u("lautta://nv-1/a"), "a", None, true).unwrap();
        f.add(&u("lautta://nv-10/a"), "b", None, true).unwrap();
        f.add(&u("lautta://nv-1/b"), "c", None, true).unwrap();
        assert_eq!(f.remove_location("nv-1").unwrap(), 2);
        let left = f.list().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].uri.location, "nv-10");
        assert_eq!(left[0].position, 0);
    }

    #[test]
    fn files_are_favourites_too_and_stay_files() {
        let f = store();
        let folder = f.add(&u("lautta://x/Uni"), "Uni", None, true).unwrap();
        let file = f
            .add(&u("lautta://x/Uni/thesis.pdf"), "Thesis", None, false)
            .unwrap();
        assert!(folder.is_dir && !file.is_dir);
        let list = f.list().unwrap();
        assert_eq!(
            list.iter()
                .map(|x| (x.label.as_str(), x.is_dir))
                .collect::<Vec<_>>(),
            [("Uni", true), ("Thesis", false)]
        );
        let got = f.get_by_uri(&u("lautta://x/Uni/thesis.pdf")).unwrap().unwrap();
        assert!(!got.is_dir);
        // The dialog relabels without knowing what the favourite is.
        let again = f
            .save(&u("lautta://x/Uni/thesis.pdf"), "Final", None, None, true)
            .unwrap();
        assert_eq!((again.id, again.is_dir), (file.id, false));
        let new = f
            .save(&u("lautta://x/notes.md"), "Notes", None, None, false)
            .unwrap();
        assert!(!new.is_dir);
        let stored = f.list().unwrap();
        assert_eq!(
            stored
                .iter()
                .map(|x| (x.label.as_str(), x.is_dir))
                .collect::<Vec<_>>(),
            [("Uni", true), ("Final", false), ("Notes", false)]
        );
        f.on_moved(&u("lautta://x/Uni"), &u("lautta://x/Old")).unwrap();
        let moved = f.get_by_uri(&u("lautta://x/Old/thesis.pdf")).unwrap().unwrap();
        assert!(!moved.is_dir, "a file favourite follows its folder");
    }

    #[test]
    fn icons_are_saved_and_loaded() {
        let f = store();
        let docs = u("lautta://x/Docs");
        let added = f.add(&docs, "Docs", None, true).unwrap();
        assert_eq!(added.icon, None, "a new favourite has the default icon");
        let saved = f.save(&docs, "Docs", Some("#f00"), Some("home"), true).unwrap();
        assert_eq!((saved.id, saved.icon.as_deref()), (added.id, Some("home")));
        let got = f.get_by_uri(&docs).unwrap().unwrap();
        assert_eq!(got.icon.as_deref(), Some("home"));
        assert_eq!(got.icon_source(), "image://theme/icon-m-home");
        assert_eq!(f.list().unwrap()[0].icon.as_deref(), Some("home"));
        let new = f
            .save(
                &u("lautta://x/Games"),
                "Games",
                None,
                Some("game-controller"),
                true,
            )
            .unwrap();
        assert_eq!(
            new.icon.as_deref(),
            Some("game-controller"),
            "added with its icon"
        );
        // Relabelling and recolouring keep the icon; saving none clears it.
        f.update(saved.id, "Papers", None).unwrap();
        assert_eq!(
            f.get_by_uri(&docs).unwrap().unwrap().icon.as_deref(),
            Some("home")
        );
        let cleared = f.save(&docs, "Papers", None, None, true).unwrap();
        assert_eq!(cleared.icon, None);
        assert_eq!(cleared.icon_source(), "image://theme/icon-m-favorite");
    }

    #[test]
    fn the_star_is_the_default_icon_of_a_folder() {
        assert_eq!(ICONS[0], "favorite", "the picker starts with the default");
        let f = store();
        let folder = f.add(&u("lautta://x/Uni"), "Uni", None, true).unwrap();
        assert_eq!(folder.default_icon(), "image://theme/icon-m-favorite");
        assert_eq!(folder.icon_source(), "image://theme/icon-m-favorite");
        // A file keeps its file's icon until another is chosen.
        let file = f.add(&u("lautta://x/Uni/a.mp3"), "A", None, false).unwrap();
        assert_eq!(file.icon_source(), "image://theme/icon-m-file-audio");
        let chosen = f
            .save(&u("lautta://x/Uni/a.mp3"), "A", None, Some("favorite"), false)
            .unwrap();
        assert_eq!(chosen.icon_source(), "image://theme/icon-m-favorite");
    }

    #[test]
    fn unknown_icons_fall_back_to_the_default() {
        let f = store();
        let docs = u("lautta://x/Docs");
        let saved = f.save(&docs, "Docs", None, Some("no-such-icon"), true).unwrap();
        assert_eq!(saved.icon, None, "an unknown icon is not stored");
        // One stored by something else (a later version) reads as none.
        f.db.lock()
            .execute("UPDATE favourites SET icon = 'heart' WHERE id = ?1", [saved.id])
            .unwrap();
        let got = f.get_by_uri(&docs).unwrap().unwrap();
        assert_eq!(got.icon, None);
        assert_eq!(got.icon_source(), "image://theme/icon-m-favorite");
        let made = Favourite {
            icon: Some("../../etc".into()),
            ..got
        };
        assert_eq!(made.icon_source(), "image://theme/icon-m-favorite");
    }

    #[test]
    fn every_icon_is_in_the_theme_once() {
        // The theme of the Sailfish OS 5.2 target.
        let theme = include_str!("../../../../doc/theme-icons.txt");
        for name in ICONS {
            let icon = format!("icon-m-{name}");
            assert!(theme.lines().any(|l| l == icon), "{icon} is not in the theme");
        }
        let mut names = ICONS.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ICONS.len());
    }

    #[test]
    fn follows_moves() {
        let f = store();
        f.add(&u("lautta://x/a/b"), "b", None, true).unwrap();
        f.on_moved(&u("lautta://x/a"), &u("lautta://x/z")).unwrap();
        assert!(f.is_favourite(&u("lautta://x/z/b")).unwrap());
    }
}
