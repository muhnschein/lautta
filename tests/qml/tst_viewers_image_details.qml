// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/viewers"

ApplicationWindow {
    id: window
    property bool reachedEnd

    ViewerTools { id: tools }

    Component.onCompleted: {
        if (!tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-pictures/details.jpg"))
            console.error("fixture not installed")
        var page = pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        for (var i = 0; i < 200 && !page.info.dateTaken; ++i)
            tools.pump(50)
        var taken = new Date(2026, 8, 12, 18, 42, 11)
        var expected = Format.formatDate(taken, Formatter.DateMedium) + " " + Format.formatDate(taken, Formatter.TimeValue)
        if (page.info.dateTaken !== "2026-09-12 18:42:11")
            console.error("the fixture's EXIF date: " + page.info.dateTaken)
        if (page.taken !== expected)
            console.error("Taken must read \"" + expected + "\", not \"" + page.taken + "\"")
        reachedEnd = true
    }

    Component {
        id: pageComponent

        ImageDetailsPage { uri: "lautta://user-pictures/details.jpg" }
    }
}
