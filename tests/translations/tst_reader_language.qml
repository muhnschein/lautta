// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6

QtObject {
    property bool reachedEnd

    function notEnglish(got, english) {
        if (got === english || got === "")
            console.error("FAILED: \"" + got + "\" is not in the reader's language")
    }

    Component.onCompleted: {
        notEnglish(qsTrId("lautta-home-title"), "Browse")
        notEnglish(qsTrId("lautta-dir-grid-items", 2), "2 items")
        if (qsTrId("lautta-app-name") !== "Lautta")
            console.error("FAILED: the app's name is translated")
        reachedEnd = true
    }
}
