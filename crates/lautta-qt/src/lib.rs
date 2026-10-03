// SPDX-License-Identifier: LGPL-2.1-or-later
//! Qt glue (SPEC ARC-3): exposes `lautta-core` to QML through qmetaobject
//! models and QObject facades. No business logic lives here.

use cpp::cpp;
use std::os::raw::{c_char, c_int};

cpp! {{
    #include <QtGui/QGuiApplication>
    #include <QtQuick/QQuickView>
    #include <QtQml/QQmlEngine>
    #include <QtQml/QQmlContext>
    #include <QtCore/QUrl>
    #include <QtCore/QCoreApplication>
#ifdef LAUTTA_SAILFISH
    #include <sailfishapp.h>
#endif
}}

/// Starts the application: Qt, the QML engine and the main page. `argv`
/// comes from the exported C `main` (SPEC RS-3), so a booster launch behaves
/// like a direct launch.
///
/// # Safety
/// `argc`/`argv` must be the process arguments as passed to `main`.
pub unsafe fn run(argc: c_int, argv: *mut *mut c_char) -> c_int {
    cpp!([argc as "int", argv as "char**"] -> c_int as "int" {
        static int count = 0;
        count = argc;
#ifdef LAUTTA_SAILFISH
        QGuiApplication *app = SailfishApp::application(count, argv);
        QQuickView *view = SailfishApp::createView();
        view->setSource(SailfishApp::pathToMainQml());
        view->show();
#else
        QGuiApplication *app = new QGuiApplication(count, argv);
        QQuickView *view = new QQuickView();
        QByteArray qml = qgetenv("LAUTTA_QML");
        view->setSource(QUrl::fromLocalFile(QString::fromLocal8Bit(qml)));
        view->show();
#endif
        int rc = app->exec();
        delete view;
        delete app;
        return rc;
    })
}
