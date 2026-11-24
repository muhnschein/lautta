// SPDX-License-Identifier: LGPL-2.1-or-later

use super::{parse_uri, qstr, run_then};
use crate::runtime::core;
use lautta_core::app_viewers::{AudioInfo, CoverFrom};
use qmetaobject::prelude::*;
use qmetaobject::qml_register_type;

pub fn register() {
    qml_register_type::<AudioInfoModel>(&crate::qml_uri(), 1, 0, &crate::cstr("AudioInfo"));
}

#[derive(QObject, Default)]
pub struct AudioInfoModel {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; WRITE setUri NOTIFY uriChanged),
    uriChanged: qt_signal!(),
    loading: qt_property!(bool; NOTIFY stateChanged),
    title: qt_property!(QString; NOTIFY stateChanged),
    artist: qt_property!(QString; NOTIFY stateChanged),
    album: qt_property!(QString; NOTIFY stateChanged),
    year: qt_property!(i32; NOTIFY stateChanged),
    track: qt_property!(i32; NOTIFY stateChanged),
    trackTotal: qt_property!(i32; NOTIFY stateChanged),
    cover: qt_property!(QString; NOTIFY stateChanged),
    stateChanged: qt_signal!(),
    generation: u64,
}

fn number(n: Option<impl Into<u64>>) -> i32 {
    n.map_or(0, |v| i32::try_from(v.into()).unwrap_or(0))
}

pub fn cover_name(c: CoverFrom) -> &'static str {
    match c {
        CoverFrom::File => "file",
        CoverFrom::Folder => "folder",
        CoverFrom::None => "",
    }
}

impl AudioInfoModel {
    fn show(&mut self, info: Option<&AudioInfo>) {
        let t = info.map(|i| &i.tags);
        let text = |s: Option<&String>| s.map_or_else(QString::default, |v| qstr(v));
        self.title = text(t.and_then(|t| t.title.as_ref()));
        self.artist = text(t.and_then(|t| t.artist.as_ref()));
        self.album = text(t.and_then(|t| t.album.as_ref()));
        self.year = number(t.and_then(|t| t.year));
        self.track = number(t.and_then(|t| t.track));
        self.trackTotal = number(t.and_then(|t| t.track_total));
        self.cover = qstr(cover_name(info.map_or(CoverFrom::None, |i| i.cover)));
    }

    fn setUri(&mut self, value: QString) {
        if self.uri == value {
            return;
        }
        self.uri = value;
        self.uriChanged();
        self.generation += 1;
        let gen = self.generation;
        self.show(None);
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            self.loading = false;
            self.stateChanged();
            return;
        };
        self.loading = true;
        self.stateChanged();
        run_then(
            self,
            async move { core.audio_info(&uri).await },
            move |me, res| {
                if me.generation != gen {
                    return;
                }
                me.loading = false;
                me.show(res.as_ref().ok());
                me.stateChanged();
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lautta_core::preview::audio::AudioTags;

    #[test]
    fn fields_are_carried_over_and_missing_ones_are_empty() {
        let mut m = AudioInfoModel::default();
        m.show(Some(&AudioInfo {
            tags: AudioTags {
                title: Some("Harbour Lights".into()),
                year: Some(2024),
                track: Some(3),
                track_total: Some(11),
                ..AudioTags::default()
            },
            cover: CoverFrom::Folder,
        }));
        assert_eq!(m.title.to_string(), "Harbour Lights");
        assert!(m.artist.to_string().is_empty() && m.album.to_string().is_empty());
        assert_eq!((m.year, m.track, m.trackTotal), (2024, 3, 11));
        assert_eq!(m.cover.to_string(), "folder");
        m.show(None);
        assert!(m.title.to_string().is_empty());
        assert_eq!((m.year, m.track, m.trackTotal), (0, 0, 0));
        assert_eq!(m.cover.to_string(), "");
    }

    #[test]
    fn cover_names() {
        assert_eq!(cover_name(CoverFrom::File), "file");
        assert_eq!(cover_name(CoverFrom::Folder), "folder");
        assert_eq!(cover_name(CoverFrom::None), "");
    }

    #[test]
    fn numbers_too_large_for_qml_are_dropped() {
        assert_eq!(number(Some(u32::MAX)), 0);
        assert_eq!(number(Some(7u32)), 7);
        assert_eq!(number(None::<u32>), 0);
    }
}
