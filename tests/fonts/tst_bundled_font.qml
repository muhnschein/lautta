// SPDX-License-Identifier: LGPL-2.1-or-later
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
