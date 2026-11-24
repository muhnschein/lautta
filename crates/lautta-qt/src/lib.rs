// SPDX-License-Identifier: LGPL-2.1-or-later
#![recursion_limit = "1024"]
#![allow(non_snake_case)]
#![allow(clippy::useless_transmute)]

pub mod app;
pub mod browse;
pub mod checksum;
pub mod directory;
pub mod icons;
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
    #include <QtCore/QCoreApplication>
    #include <QtCore/QDir>
    #include <QtCore/QLocale>
    #include <QtCore/QTranslator>
    #include <QtGui/QFontDatabase>

    static bool lauttaLoadFonts(const QString &directory)
    {
        bool loaded = false;
        const QDir dir(directory);
        for (const QString &name : dir.entryList(QStringList(QStringLiteral("*.ttf")), QDir::Files))
            loaded = QFontDatabase::addApplicationFont(dir.filePath(name)) >= 0 || loaded;
        return loaded;
    }

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
}}

cpp! {{
    #include <QtGui/QGuiApplication>
    #include <QtQuick/QQuickView>
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
        QString fonts;
        const QStringList args = QCoreApplication::arguments().mid(2);
        for (int i = 0; i < args.size(); ++i) {
            if (args.at(i) == QLatin1String("-I") && i + 1 < args.size())
                importPaths << args.at(++i);
            else if (args.at(i) == QLatin1String("-T") && i + 1 < args.size())
                translations = args.at(++i);
            else if (args.at(i) == QLatin1String("-F") && i + 1 < args.size())
                fonts = args.at(++i);
            else
                files << args.at(i);
        }
        if (!translations.isEmpty() && !lauttaInstallTranslations(&app, translations)) {
            std::printf("ERROR   no harbour-lautta.qm in %s\n", qPrintable(translations));
            return 1;
        }
        if (!fonts.isEmpty() && !lauttaLoadFonts(fonts)) {
            std::printf("ERROR   no font loads from %s\n", qPrintable(fonts));
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

/// # Safety
/// `argc`/`argv` must be the process arguments as passed to `main`.
pub unsafe fn run(argc: c_int, argv: *mut *mut c_char) -> c_int {
    lautta_core::panics::abort_outside_catch();
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
            if (!lauttaLoadFonts(SailfishApp::pathTo(QStringLiteral("fonts")).toLocalFile()))
                qWarning("bundled fonts missing");
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

fn open_core() {
    let paths = lautta_core::paths::AppPaths::from_env();
    if let Err(e) = runtime::init(paths) {
        log::error!("cannot open the app data: {e}");
    }
}

pub fn register_types() {
    qmetaobject::qml_register_singleton_type::<app::App>(&qml_uri(), 1, 0, &cstr("App"));
    browse::register();
    checksum::register();
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

pub fn cstr(s: &str) -> std::ffi::CString {
    std::ffi::CString::new(s).unwrap_or_default()
}

pub fn qml_check(args: &[String]) -> c_int {
    open_core();
    register_types();
    let mut owned: Vec<std::ffi::CString> = vec![cstr("harbour-lautta"), cstr("--qml-check")];
    owned.extend(args.iter().map(|a| cstr(a)));
    let mut ptrs: Vec<*mut c_char> = owned.iter().map(|c| c.as_ptr() as *mut c_char).collect();
    ptrs.push(std::ptr::null_mut());
    let argc = c_int::try_from(owned.len()).unwrap_or(c_int::MAX);
    let argv = ptrs.as_mut_ptr();
    unsafe {
        cpp!([argc as "int", argv as "char**"] -> c_int as "int" { return lauttaQmlCheck(argc, argv); })
    }
}
