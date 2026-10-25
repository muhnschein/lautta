// SPDX-License-Identifier: LGPL-2.1-or-later
//! `ViewerTools`: small helpers the viewer pages need (Recents notes, ORG-2)
//! and some for the QML tests.

use super::parse_uri;
use crate::runtime::core;
use cpp::cpp;
use lautta_core::org::recents::RecentKind;
use qmetaobject::prelude::*;
use qmetaobject::qml_register_type;
use std::path::PathBuf;

cpp! {{
    #include <QtCore/QCoreApplication>
    #include <QtCore/QElapsedTimer>
    #include <QtCore/QEventLoop>
    #include <QtCore/QTimer>
    #include <QtGui/QMouseEvent>
    #include <QtQuick/QQuickItem>
    #include <QtQuick/QQuickWindow>
}}

pub fn register() {
    qml_register_type::<ViewerTools>(&crate::qml_uri(), 1, 0, &crate::cstr("ViewerTools"));
}

#[derive(QObject, Default)]
pub struct ViewerTools {
    base: qt_base_class!(trait QObject),
    /// Notes a file the user looked at in Recents (previewed).
    noteViewed: qt_method!(fn(&self, uri: QString)),
    /// Notes a file handed to another app in Recents (opened).
    noteOpened: qt_method!(fn(&self, uri: QString)),
    /// Test support: runs the event loop for `ms` milliseconds so async
    /// results of the viewer types arrive while a test is running.
    pump: qt_method!(fn(&self, ms: i32)),
    /// Test support: copies the local file `sourceUrl` (a `file://` URL) to
    /// the local location path of `destUri`; true when it worked.
    installFixture: qt_method!(fn(&self, source_url: QString, dest_uri: QString) -> bool),
    /// Test support: sets the modified time of the local item `uri` to `ms`
    /// milliseconds after the epoch; true when it worked.
    setModified: qt_method!(fn(&self, uri: QString, ms: f64) -> bool),
    /// Test support: a finger's `kind` ("press", "move" or "release") at
    /// (`x`, `y`) of `item`, sent to its window as the mouse event Qt makes of
    /// a touch, so gestures run through the real mouse areas and flickables.
    /// An item's tree without a window gets one first. False when `item` is
    /// not an item or `kind` is unknown.
    mouse: qt_method!(fn(&self, item: QVariant, kind: QString, x: f64, y: f64) -> bool),
}

impl ViewerTools {
    fn note(&self, uri: QString, kind: RecentKind) {
        if let (Some(core), Some(u)) = (core(), parse_uri(&uri)) {
            core.note_viewed(&u, kind);
        }
    }

    fn noteViewed(&self, uri: QString) {
        self.note(uri, RecentKind::Previewed);
    }

    fn noteOpened(&self, uri: QString) {
        self.note(uri, RecentKind::Opened);
    }

    fn pump(&self, ms: i32) {
        cpp!(unsafe [ms as "int"] {
            QEventLoop loop;
            QTimer::singleShot(ms, &loop, &QEventLoop::quit);
            loop.exec();
        });
    }

    fn mouse(&self, item: QVariant, kind: QString, x: f64, y: f64) -> bool {
        let kind: i32 = match kind.to_string().as_str() {
            "press" => 0,
            "move" => 1,
            "release" => 2,
            _ => return false,
        };
        let item = &item;
        cpp!(unsafe [item as "const QVariant*", kind as "int", x as "double", y as "double"] -> bool as "bool" {
            QQuickItem *target = qobject_cast<QQuickItem *>(qvariant_cast<QObject *>(*item));
            if (!target)
                return false;
            QQuickWindow *window = target->window();
            if (!window) {
                // The QML checks create their scenes without a window (the app's
                // comes from SailfishApp): the item's whole tree gets one.
                QQuickItem *root = target;
                while (root->parentItem())
                    root = root->parentItem();
                window = new QQuickWindow;
                window->resize(qMax(1, qRound(root->width())), qMax(1, qRound(root->height())));
                root->setParentItem(window->contentItem());
                QObject::connect(root, &QObject::destroyed, window, &QObject::deleteLater);
            }
            // Events carry increasing times: flickables measure speed by them.
            static QElapsedTimer clock;
            if (!clock.isValid())
                clock.start();
            const QPointF scene = target->mapToScene(QPointF(x, y));
            const QEvent::Type type = kind == 0 ? QEvent::MouseButtonPress
                                    : kind == 1 ? QEvent::MouseMove : QEvent::MouseButtonRelease;
            const Qt::MouseButton button = kind == 1 ? Qt::NoButton : Qt::LeftButton;
            const Qt::MouseButtons held = kind == 2 ? Qt::NoButton : Qt::LeftButton;
            QMouseEvent event(type, scene, scene, window->mapToGlobal(scene.toPoint()), button, held,
                              Qt::NoModifier);
            event.setTimestamp(static_cast<ulong>(clock.elapsed()) + 1);
            QCoreApplication::sendEvent(window, &event);
            return true;
        })
    }

    fn setModified(&self, uri: QString, ms: f64) -> bool {
        let (Some(core), Some(item)) = (core(), parse_uri(&uri)) else {
            return false;
        };
        let Some(path) = core.locations.to_local_path(&item) else {
            return false;
        };
        let when = std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms as u64);
        std::fs::File::open(path)
            .and_then(|file| file.set_modified(when))
            .is_ok()
    }

    fn installFixture(&self, source_url: QString, dest_uri: QString) -> bool {
        let (Some(core), Some(dest)) = (core(), parse_uri(&dest_uri)) else {
            return false;
        };
        let source = source_url.to_string();
        let Some(source) = source.strip_prefix("file://").map(PathBuf::from) else {
            return false;
        };
        match core.locations.to_local_path(&dest) {
            Some(target) => std::fs::copy(source, target).is_ok(),
            None => false,
        }
    }
}
