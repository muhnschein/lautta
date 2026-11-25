// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6

QtObject {
    property bool scenarioRunning: true
    Component.onCompleted: console.log("canary waits for its crash")
}
