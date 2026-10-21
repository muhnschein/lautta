// SPDX-License-Identifier: LGPL-2.1-or-later
// The image details panel (PRV-4, board ImageExif) on the target: the date
// the photo was taken follows the system's date and time formats like every
// date in the app (§18), not the "2026-09-12 18:42:11" EXIF records. Fails
// when the raw EXIF text is shown.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/viewers"

ApplicationWindow {
    id: window

    ViewerTools { id: tools }

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/photo.jpg"), "lautta://user-pictures/details.jpg")
        pageStack.push(pageComponent, {}, PageStackAction.Immediate)
        tools.pump(500)
        var page = pageStack.currentPage
        var taken = new Date(2026, 8, 12, 18, 42, 11)
        var expected = Format.formatDate(taken, Formatter.DateMedium) + " " + Format.formatDate(taken, Formatter.TimeValue)
        if (page.info.dateTaken !== "2026-09-12 18:42:11")
            console.error("the fixture's EXIF date: " + page.info.dateTaken)
        if (page.taken !== expected)
            console.error("Taken must read \"" + expected + "\", not \"" + page.taken + "\"")
    }

    Component {
        id: pageComponent

        ImageDetailsPage { uri: "lautta://user-pictures/details.jpg" }
    }
}
