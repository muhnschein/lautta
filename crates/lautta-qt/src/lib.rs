// SPDX-License-Identifier: LGPL-2.1-or-later
//! Qt glue: exposes `lautta-core` to QML through qmetaobject models and QObject facades. No
//! business logic.
// C++ blocks are large for the `cpp!` macro expansion.
#![recursion_limit = "1024"]
#![allow(non_snake_case)]
// qmetaobject derives expand to transmutes clippy flags.
#![allow(clippy::useless_transmute)]

pub mod app;
pub mod browse;
pub mod directory;
pub mod json;
pub mod operations;
pub mod runtime;
pub mod scan;
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
    #include <QtCore/QElapsedTimer>
    #include <QtCore/QFileInfo>
    #include <QtCore/QEvent>
    #include <QtCore/QEventLoop>
    #include <QtQml/QQmlComponent>
    #include <QtCore/QTranslator>
    #include <QtCore/QLocale>
    #include <QtCore/QLibraryInfo>
    #include <QtCore/QRegularExpression>
    #include <cstdio>
    #include <memory>
#ifdef LAUTTA_SAILFISH
    #include <sailfishapp.h>
#endif

    // Installs `directory`'s catalogues: engineering English first, then user's language. Last
    // installed is asked first, so a missing message falls back to English (has plural forms).
    // False when the catalogue is missing.
    static bool lauttaInstallTranslations(QCoreApplication *app, const QString &directory)
    {
        QTranslator *english = new QTranslator(app);
        const bool found = english->load(QStringLiteral("harbour-lautta"), directory);
        if (found)
            app->installTranslator(english);
        QTranslator *local = new QTranslator(app);
        if (local->load(QLocale(), QStringLiteral("harbour-lautta"), QStringLiteral("-"), directory))
            app->installTranslator(local);
        return found;
    }

    // QML check mode: compiles and instantiates QML documents with this binary's engine (real
    // Silica, real Lautta types). Any error, any warning or console.error naming a checked
    // document, and any uncaught JS exception outside Qt's modules, fails. A `tst_*` document
    // fails unless its root `reachedEnd` is true at the end. `-T <directory>` installs that
    // directory's catalogues first; without it qsTrId gives back the id.
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

    // Uncaught JS exception outside Qt's QML modules, e.g. from a `.js` library a checked document
    // uses. Engine names throw site, not caller: file name alone misses it. Silica (a Qt module)
    // throws on pages loaded without ApplicationWindow: not ours.
    static bool lauttaThrown(const QQmlError &e)
    {
        static const QRegularExpression thrown(QStringLiteral("^[A-Za-z]*Error: "));
        static const QString qt = QLibraryInfo::location(QLibraryInfo::Qml2ImportsPath) + QLatin1Char('/');
        return thrown.match(e.description()).hasMatch() && !e.url().toLocalFile().startsWith(qt);
    }

    static int lauttaQmlCheck(int argc, char **argv)
    {
        static int count = 0;
        count = argc;
        QGuiApplication app(count, argv);
        QStringList importPaths;
        QStringList files;
        QString translations;
        const QStringList args = QCoreApplication::arguments().mid(2);
        for (int i = 0; i < args.size(); ++i) {
            if (args.at(i) == QLatin1String("-I") && i + 1 < args.size())
                importPaths << args.at(++i);
            else if (args.at(i) == QLatin1String("-T") && i + 1 < args.size())
                translations = args.at(++i);
            else
                files << args.at(i);
        }
        if (!translations.isEmpty() && !lauttaInstallTranslations(&app, translations)) {
            std::printf("ERROR   no harbour-lautta.qm in %s\n", qPrintable(translations));
            return 1;
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
                    if (e.url().toLocalFile() == file || e.toString().contains(file) || lauttaThrown(e))
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
            // `scenarioRunning` stays true until the steps are done; failure = logged error.
            if (object && object->property("scenarioRunning").isValid()) {
                QElapsedTimer clock;
                clock.start();
                while (object->property("scenarioRunning").toBool() && clock.elapsed() < 60000) {
                    QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
                    QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
                }
                if (object->property("scenarioRunning").toBool())
                    problems << QStringLiteral("scenario still running after 60 s");
            }
            if (object && QFileInfo(file).fileName().startsWith(QLatin1String("tst_"))
                && !object->property("reachedEnd").toBool())
                problems << QStringLiteral("checks unproven: root reachedEnd is not true");
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

/// `argv` from the exported C `main`. Panics abort, apart from PDF drawing's; tests starting the
/// QML engine via [`qml_check`] keep the default hook.
///
/// # Safety
/// `argc`/`argv` must be the process arguments as passed to `main`.
pub unsafe fn run(argc: c_int, argv: *mut *mut c_char) -> c_int {
    lautta_core::panics::abort_outside_catch();
    // Core (database, locations, bridge) opens while Qt starts; QML waits for it, not Qt.
    *lock_opening() = Some(std::thread::spawn(open_core));
    register_types();
    cpp!([argc as "int", argv as "char**"] -> c_int as "int" {
            if (argc > 1 && qstrcmp(argv[1], "--qml-check") == 0) {
                rust!(LauttaWaitCoreCheck [] { wait_core() });
                return lauttaQmlCheck(argc, argv);
            }
            static int count = 0;
            count = argc;
    #ifdef LAUTTA_SAILFISH
            QGuiApplication *app = SailfishApp::application(count, argv);
            lauttaInstallTranslations(app, SailfishApp::pathTo(QStringLiteral("translations")).toLocalFile());
            QQuickView *view = SailfishApp::createView();
            lauttaRegisterImageProviders(view->engine());
            rust!(LauttaWaitCore [] { wait_core() });
            view->setSource(SailfishApp::pathToMainQml());
            view->show();
    #else
            QGuiApplication *app = new QGuiApplication(count, argv);
            QQuickView *view = new QQuickView();
            lauttaRegisterImageProviders(view->engine());
            rust!(LauttaWaitCoreHost [] { wait_core() });
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

pub const QML_URI: &str = "Lautta";

static OPENING: std::sync::Mutex<Option<std::thread::JoinHandle<()>>> = std::sync::Mutex::new(None);

fn lock_opening() -> std::sync::MutexGuard<'static, Option<std::thread::JoinHandle<()>>> {
    OPENING.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn wait_core() {
    let opening = lock_opening().take();
    if let Some(t) = opening {
        let _ = t.join();
    }
}

/// Failure to open core is shown by the start page via `App.startError`.
fn open_core() {
    let paths = lautta_core::paths::AppPaths::from_env();
    if let Err(e) = runtime::init(paths) {
        log::error!("cannot open the app data: {e}");
    }
}

pub fn register_types() {
    qmetaobject::qml_register_singleton_type::<app::App>(&qml_uri(), 1, 0, &cstr("App"));
    browse::register();
    directory::register();
    operations::register();
    scan::register();
    transfers::register();
    viewers::register();
    search::register();
}

pub fn qml_uri() -> std::ffi::CString {
    cstr(QML_URI)
}

/// No interior NUL in our names.
pub fn cstr(s: &str) -> std::ffi::CString {
    std::ffi::CString::new(s).unwrap_or_default()
}

/// `-I <path>`, `-T <catalogue folder>` and files. Opens core from `$HOME` and registers types like
/// the app. For host QML tests.
pub fn qml_check(args: &[String]) -> c_int {
    open_core();
    register_types();
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
