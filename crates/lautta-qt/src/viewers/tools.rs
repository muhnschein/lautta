// SPDX-License-Identifier: LGPL-2.1-or-later
//! Helpers for the viewer pages (Recents notes) and QML tests.

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
    noteViewed: qt_method!(fn(&self, uri: QString)),
    noteOpened: qt_method!(fn(&self, uri: QString)),
    /// Test support: pumps the event loop so async viewer results arrive during a test.
    pump: qt_method!(fn(&self, ms: i32)),
    /// Test support: copies local `sourceUrl` (`file://`) to the local path of `destUri`.
    installFixture: qt_method!(fn(&self, source_url: QString, dest_uri: QString) -> bool),
    /// Test support: sets mtime of local `uri` to `ms` since epoch.
    setModified: qt_method!(fn(&self, uri: QString, ms: f64) -> bool),
    /// Test support: finger `kind` ("press", "move", "release") at (`x`, `y`) of `item`, sent as
    /// the mouse event Qt makes of a touch, so gestures run through real mouse areas and
    /// flickables. Gives a windowless tree a window first.
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
                // QML checks create scenes without a window (the app's comes from SailfishApp).
                QQuickItem *root = target;
                while (root->parentItem())
                    root = root->parentItem();
                window = new QQuickWindow;
                window->resize(qMax(1, qRound(root->width())), qMax(1, qRound(root->height())));
                root->setParentItem(window->contentItem());
                QObject::connect(root, &QObject::destroyed, window, &QObject::deleteLater);
            }
            // Event times: wall clock, at least one touch frame (16 ms) after the last. Qt 5.6 on
            // target: a move in the same millisecond as the one before scrolls no flickable until a
            // later-timed move, so drag length would follow runner speed. Flickables measure speed
            // by these times.
            static QElapsedTimer clock;
            static qint64 last = 0;
            if (!clock.isValid())
                clock.start();
            last = qMax(clock.elapsed() + 1, last + 16);
            const QPointF scene = target->mapToScene(QPointF(x, y));
            const QEvent::Type type = kind == 0 ? QEvent::MouseButtonPress
                                    : kind == 1 ? QEvent::MouseMove : QEvent::MouseButtonRelease;
            const Qt::MouseButton button = kind == 1 ? Qt::NoButton : Qt::LeftButton;
            const Qt::MouseButtons held = kind == 2 ? Qt::NoButton : Qt::LeftButton;
            QMouseEvent event(type, scene, scene, window->mapToGlobal(scene.toPoint()), button, held,
                              Qt::NoModifier);
            event.setTimestamp(static_cast<ulong>(last));
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
