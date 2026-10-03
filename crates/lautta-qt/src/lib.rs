// SPDX-License-Identifier: LGPL-2.1-or-later
//! Qt glue (SPEC ARC-3): exposes `lautta-core` to QML through qmetaobject
//! models and QObject facades. No business logic lives here.
// The C++ blocks are large for the `cpp!` macro expansion.
#![recursion_limit = "1024"]

use cpp::cpp;
use std::os::raw::{c_char, c_int};

cpp! {{
    #include <QtGui/QGuiApplication>
    #include <QtQuick/QQuickView>
    #include <QtQml/QQmlEngine>
    #include <QtQml/QQmlContext>
    #include <QtCore/QUrl>
    #include <QtCore/QCoreApplication>
    #include <QtQml/QQmlComponent>
    #include <cstdio>
    #include <memory>
#ifdef LAUTTA_SAILFISH
    #include <sailfishapp.h>
#endif

    // QML check mode (SPEC TST-6, tests/qml): compiles and instantiates QML
    // documents with this binary's engine, so the real Silica modules and the
    // real Lautta types are used. Any error, and any warning or console.error
    // naming a checked document, fails.
    static QStringList *lauttaCheckProblems = nullptr;
    static QString lauttaCheckFile;

    static void lauttaCheckMessage(QtMsgType type, const QMessageLogContext &context, const QString &msg)
    {
        const QString where = QString::fromUtf8(context.file ? context.file : "");
        const bool ours = where.contains(lauttaCheckFile) || msg.contains(lauttaCheckFile);
        if (lauttaCheckProblems && type != QtDebugMsg && type != QtInfoMsg && ours)
            *lauttaCheckProblems << (where + QStringLiteral(": ") + msg);
        else
            std::fprintf(stderr, "note    %s\n", qPrintable(msg));
    }

    static int lauttaQmlCheck(int argc, char **argv)
    {
        static int count = 0;
        count = argc;
        QGuiApplication app(count, argv);
        QStringList importPaths;
        QStringList files;
        const QStringList args = QCoreApplication::arguments().mid(2);
        for (int i = 0; i < args.size(); ++i) {
            if (args.at(i) == QLatin1String("-I") && i + 1 < args.size())
                importPaths << args.at(++i);
            else
                files << args.at(i);
        }
        int failures = 0;
        for (const QString &file : files) {
            QQmlEngine engine;
            for (const QString &path : importPaths)
                engine.addImportPath(path);
            QStringList problems;
            QObject::connect(&engine, &QQmlEngine::warnings, [&problems, &file](const QList<QQmlError> &list) {
                for (const QQmlError &e : list) {
                    if (e.url().toLocalFile() == file || e.toString().contains(file))
                        problems << e.toString();
                    else
                        std::printf("note    %s\n", qPrintable(e.toString()));
                }
            });
            lauttaCheckProblems = &problems;
            lauttaCheckFile = file;
            QtMessageHandler previous = qInstallMessageHandler(lauttaCheckMessage);
            QQmlComponent component(&engine, QUrl::fromLocalFile(file), QQmlComponent::PreferSynchronous);
            while (component.isLoading())
                QCoreApplication::processEvents();
            std::unique_ptr<QObject> object(component.isReady() ? component.create() : nullptr);
            for (int spin = 0; spin < 10; ++spin)
                QCoreApplication::processEvents();
            for (const QQmlError &e : component.errors())
                problems << e.toString();
            if (!object && problems.isEmpty())
                problems << QStringLiteral("could not be created");
            qInstallMessageHandler(previous);
            lauttaCheckProblems = nullptr;
            for (const QString &p : problems)
                std::printf("ERROR   %s\n", qPrintable(p));
            std::printf("%s %s\n", problems.isEmpty() ? "ok     " : "FAILED ", qPrintable(file));
            if (!problems.isEmpty())
                ++failures;
        }
        std::printf("%d of %d files failed\n", failures, static_cast<int>(files.size()));
        return failures ? 1 : 0;
    }
}}

/// Starts the application: Qt, the QML engine and the main page. `argv`
/// comes from the exported C `main` (SPEC RS-3), so a booster launch behaves
/// like a direct launch.
///
/// # Safety
/// `argc`/`argv` must be the process arguments as passed to `main`.
pub unsafe fn run(argc: c_int, argv: *mut *mut c_char) -> c_int {
    cpp!([argc as "int", argv as "char**"] -> c_int as "int" {
            if (argc > 1 && qstrcmp(argv[1], "--qml-check") == 0)
                return lauttaQmlCheck(argc, argv);
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
