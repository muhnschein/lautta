// SPDX-License-Identifier: LGPL-2.1-or-later
//! `ExifModel`: the fields of the image details panel (PRV-4, board
//! ImageExif). Values are raw; QML formats and translates them.

use super::{parse_uri, qstr, run_then};
use crate::json::to_json;
use crate::runtime::core;
use lautta_core::app_viewers::ExifReport;
use qmetaobject::prelude::*;
use qmetaobject::qml_register_type;

pub fn register() {
    qml_register_type::<ExifModel>(&crate::qml_uri(), 1, 0, &crate::cstr("ExifModel"));
}

/// `{name, size, modifiedMs, hasExif, make, model, lens, dateTaken,
/// exposure, aperture, iso, focalLengthMm, width, height, orientation,
/// latitude, longitude}`; fields the file lacks are `null`.
pub fn report_json(r: &ExifReport) -> String {
    let i = r.info.as_ref();
    to_json(&serde_json::json!({
        "name": r.name,
        "size": r.size,
        "modifiedMs": r.modified_ms,
        "hasExif": i.is_some(),
        "make": i.and_then(|i| i.make.clone()),
        "model": i.and_then(|i| i.model.clone()),
        "lens": i.and_then(|i| i.lens.clone()),
        "dateTaken": i.and_then(|i| i.date_taken.clone()),
        "exposure": i.and_then(|i| i.exposure.clone()),
        "aperture": i.and_then(|i| i.aperture.clone()),
        "iso": i.and_then(|i| i.iso),
        "focalLengthMm": i.and_then(|i| i.focal_length_mm),
        "width": i.and_then(|i| i.width),
        "height": i.and_then(|i| i.height),
        "orientation": i.and_then(|i| i.orientation),
        "latitude": i.and_then(|i| i.gps.map(|g| g.0)),
        "longitude": i.and_then(|i| i.gps.map(|g| g.1)),
    }))
}

#[derive(QObject, Default)]
pub struct ExifModel {
    base: qt_base_class!(trait QObject),
    uri: qt_property!(QString; WRITE setUri NOTIFY uriChanged),
    uriChanged: qt_signal!(),
    loading: qt_property!(bool; NOTIFY stateChanged),
    /// JSON object, see `report_json`; empty until loaded.
    infoJson: qt_property!(QString; NOTIFY stateChanged),
    hasExif: qt_property!(bool; NOTIFY stateChanged),
    errorKind: qt_property!(QString; NOTIFY stateChanged),
    errorMessage: qt_property!(QString; NOTIFY stateChanged),
    stateChanged: qt_signal!(),
    generation: u64,
}

impl ExifModel {
    fn setUri(&mut self, value: QString) {
        if self.uri == value {
            return;
        }
        self.uri = value;
        self.uriChanged();
        self.generation += 1;
        let gen = self.generation;
        self.infoJson = QString::default();
        self.hasExif = false;
        self.errorKind = QString::default();
        let (Some(core), Some(uri)) = (core(), parse_uri(&self.uri)) else {
            self.stateChanged();
            return;
        };
        self.loading = true;
        self.stateChanged();
        run_then(
            self,
            async move { core.exif_report(&uri).await },
            move |me, res| {
                if me.generation != gen {
                    return;
                }
                me.loading = false;
                match res {
                    Ok(r) => {
                        me.hasExif = r.info.is_some();
                        me.infoJson = QString::from(report_json(&r).as_str());
                    }
                    Err(e) => {
                        me.errorKind = qstr(e.kind.name());
                        me.errorMessage = qstr(&e.message);
                    }
                }
                me.stateChanged();
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lautta_core::preview::exif::ExifInfo;

    #[test]
    fn missing_fields_are_null() {
        let r = ExifReport {
            name: "a.jpg".into(),
            size: Some(10),
            modified_ms: None,
            info: None,
        };
        let v: serde_json::Value = serde_json::from_str(&report_json(&r)).unwrap();
        assert_eq!(v["hasExif"], false);
        assert!(v["make"].is_null() && v["modifiedMs"].is_null());
        assert_eq!(v["size"], 10);
    }

    #[test]
    fn fields_are_carried_over() {
        let info = ExifInfo {
            make: Some("Jolla".into()),
            iso: Some(50),
            orientation: Some(6),
            gps: Some((60.15, 24.95)),
            ..ExifInfo::default()
        };
        let r = ExifReport {
            name: "a.jpg".into(),
            size: None,
            modified_ms: Some(5),
            info: Some(info),
        };
        let v: serde_json::Value = serde_json::from_str(&report_json(&r)).unwrap();
        assert_eq!(v["hasExif"], true);
        assert_eq!(v["make"], "Jolla");
        assert_eq!(v["iso"], 50);
        assert_eq!(v["orientation"], 6);
        assert_eq!(v["latitude"], 60.15);
        assert_eq!(v["longitude"], 24.95);
    }
}
