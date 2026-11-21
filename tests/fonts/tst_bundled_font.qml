// SPDX-License-Identifier: LGPL-2.1-or-later
// Run with `--qml-check -F <folder>` by tools/ci/check-rpm.sh (target, installed package).
import QtQuick 2.6
import Lautta 1.0

QtObject {
    property bool reachedEnd

    Component.onCompleted: {
        if (Qt.fontFamilies().indexOf(App.monoFont) < 0)
            console.error("FAILED: " + App.monoFont + " not among " + Qt.fontFamilies().join(", "))
        reachedEnd = true
    }
}
