// SPDX-License-Identifier: LGPL-2.1-or-later
//! Qt glue (SPEC ARC-3): exposes `lautta-core` to QML through qmetaobject
//! models and QObject facades. No business logic lives here.
// The C++ blocks are large for the `cpp!` macro expansion.
#![recursion_limit = "1024"]
// QML-facing names (properties, methods, signals) follow QML's camelCase.
#![allow(non_snake_case)]
// The qmetaobject derive macros expand to transmutes clippy flags.
#![allow(clippy::useless_transmute)]

pub mod app;
pub mod browse;
pub mod crash;
pub mod directory;
pub mod json;
pub mod operations;
pub mod runtime;
pub mod search;
pub mod transfers;
pub mod viewers;

use cpp::cpp;
use std::os::raw::{c_char, c_int};

cpp! {{
    #include <QtGui/QGuiApplication>
    #include <QtQuick/QQuickView>
    // Defined in viewers/images.rs (all cpp! blocks share one translation unit).
    static void lauttaRegisterImageProviders(QQmlEngine *engine);
    #include <QtQml/QQmlEngine>
    #include <QtQml/QQmlContext>
    #include <QtCore/QUrl>
    #include <QtCore/QCoreApplication>
    #include <QtQml/QQmlComponent>
    #include <QtCore/QTranslator>
    #include <QtCore/QLocale>
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
            lauttaRegisterImageProviders(&engine);
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
/// comes from the exported C `main` (SPEC RS-3).
///
/// # Safety
/// `argc`/`argv` must be the process arguments as passed to `main`.
pub unsafe fn run(argc: c_int, argv: *mut *mut c_char) -> c_int {
    start();
    cpp!([argc as "int", argv as "char**"] -> c_int as "int" {
            if (argc > 1 && qstrcmp(argv[1], "--qml-check") == 0)
                return lauttaQmlCheck(argc, argv);
            static int count = 0;
            count = argc;
    #ifdef LAUTTA_SAILFISH
            QGuiApplication *app = SailfishApp::application(count, argv);
            // Engineering English first, then the user's language (UI-7).
            const QString translations = SailfishApp::pathTo(QStringLiteral("translations")).toLocalFile();
            QTranslator *english = new QTranslator(app);
            if (english->load(QStringLiteral("harbour-lautta"), translations))
                app->installTranslator(english);
            QTranslator *local = new QTranslator(app);
            if (local->load(QLocale(), QStringLiteral("harbour-lautta"), QStringLiteral("-"), translations))
                app->installTranslator(local);
            QQuickView *view = SailfishApp::createView();
            lauttaRegisterImageProviders(view->engine());
            view->setSource(SailfishApp::pathToMainQml());
            view->show();
    #else
            QGuiApplication *app = new QGuiApplication(count, argv);
            QQuickView *view = new QQuickView();
            lauttaRegisterImageProviders(view->engine());
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

/// QML module URI of the app's types.
pub const QML_URI: &str = "Lautta";

/// Opens the core (SPEC §5) and registers the QML types. A failure to open
/// the core is shown by the start page through `App.startError`.
fn start() {
    let paths = lautta_core::paths::AppPaths::from_env();
    lautta_core::crash::install(paths.crash_dir(), env!("CARGO_PKG_VERSION"));
    if let Err(e) = runtime::init(paths) {
        log::error!("cannot open the app data: {e}");
    }
    register_types();
}

/// Registers every QML type under `Lautta 1.0` (doc/QML-API.md).
pub fn register_types() {
    qmetaobject::qml_register_singleton_type::<app::App>(&qml_uri(), 1, 0, &cstr("App"));
    browse::register();
    directory::register();
    operations::register();
    transfers::register();
    viewers::register();
    search::register();
}

/// `QML_URI` as a C string for the registration functions.
pub fn qml_uri() -> std::ffi::CString {
    cstr(QML_URI)
}

/// A C string from a type name literal (no interior NUL in our names).
pub fn cstr(s: &str) -> std::ffi::CString {
    std::ffi::CString::new(s).unwrap_or_default()
}

/// Runs the QML check (see `--qml-check`) in this process with the given
/// arguments (`-I <path>` and files). Opens the core from `$HOME` and
/// registers the types first, like the app does. Used by the host QML tests.
pub fn qml_check(args: &[String]) -> c_int {
    start();
    let mut owned: Vec<std::ffi::CString> = vec![cstr("harbour-lautta"), cstr("--qml-check")];
    owned.extend(args.iter().map(|a| cstr(a)));
    let mut ptrs: Vec<*mut c_char> = owned.iter().map(|c| c.as_ptr() as *mut c_char).collect();
    ptrs.push(std::ptr::null_mut());
    let argc = c_int::try_from(owned.len()).unwrap_or(c_int::MAX);
    let argv = ptrs.as_mut_ptr();
    // SAFETY: argv points to NUL-terminated strings that outlive the call.
    unsafe {
        cpp!([argc as "int", argv as "char**"] -> c_int as "int" { return lauttaQmlCheck(argc, argv); })
    }
}
