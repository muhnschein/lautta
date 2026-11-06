// SPDX-License-Identifier: LGPL-2.1-or-later
// Must fail: proves a reported failure fails the check on the target.
import QtQuick 2.6

QtObject {
    property bool reachedEnd
    Component.onCompleted: {
        console.error("FAILED: the canary reports a failure")
        reachedEnd = true
    }
}
