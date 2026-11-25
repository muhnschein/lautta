// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6

QtObject {
    property bool reachedEnd
    Component.onCompleted: {
        console.error("FAILED: the canary reports a failure")
        reachedEnd = true
    }
}
