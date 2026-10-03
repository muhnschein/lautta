// SPDX-License-Identifier: LGPL-2.1-or-later
//! Compiles the C++ glue in `cpp!` blocks. Qt is located through
//! QT_INCLUDE_PATH/QT_LIBRARY_PATH (set by the rpm build, where build scripts
//! cannot run the target's qmake under scratchbox2) or `qmake -query` on the
//! host. With the `sailfish` feature the glue uses libsailfishapp.

use std::process::Command;

fn qmake_query(var: &str) -> Option<String> {
    let qmake = std::env::var("QMAKE").unwrap_or_else(|_| "qmake".to_owned());
    let out = Command::new(qmake).args(["-query", var]).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn qt_path(env: &str, query: &str) -> String {
    std::env::var(env)
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| qmake_query(query))
        .unwrap_or_else(|| panic!("set {env} or put qmake in PATH"))
}

fn main() {
    let include = qt_path("QT_INCLUDE_PATH", "QT_INSTALL_HEADERS");
    let libs = qt_path("QT_LIBRARY_PATH", "QT_INSTALL_LIBS");
    let sailfish = std::env::var("CARGO_FEATURE_SAILFISH").is_ok();

    let mut config = cpp_build::Config::new();
    config.flag("-std=c++11");
    for module in ["", "QtCore", "QtGui", "QtQml", "QtQuick", "QtMultimedia"] {
        config.include(if module.is_empty() {
            include.clone()
        } else {
            format!("{include}/{module}")
        });
    }
    if sailfish {
        config.include("/usr/include/sailfishapp");
        config.define("LAUTTA_SAILFISH", None);
    }
    config.build("src/lib.rs");

    println!("cargo:rustc-link-search={libs}");
    for lib in ["Qt5Core", "Qt5Gui", "Qt5Qml", "Qt5Quick", "Qt5Multimedia"] {
        println!("cargo:rustc-link-lib={lib}");
    }
    if sailfish {
        println!("cargo:rustc-link-lib=sailfishapp");
    }
    println!("cargo:rerun-if-env-changed=QT_INCLUDE_PATH");
    println!("cargo:rerun-if-env-changed=QT_LIBRARY_PATH");
    println!("cargo:rerun-if-changed=src");
}
