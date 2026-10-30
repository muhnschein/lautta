// SPDX-License-Identifier: LGPL-2.1-or-later Run with `--qml-check -T <folder>` by
// crates/lautta-qt/tests/translations.rs (host) and tools/ci/check-rpm.sh (target).
import QtQuick 2.6

QtObject {
    // Only the first difference is reported: a wrong catalogue is wrong in many places.
    property bool failed: false

    function same(got, want, what) {
        if (got === want || failed)
            return
        failed = true
        console.error("FAILED: " + what + ": \"" + got + "\", not \"" + want + "\"")
    }

    function plural(id, one, other) {
        same(qsTrId(id, 1), one, id + " for 1")
        same(qsTrId(id, 2), other.replace("%n", "2"), id + " for 2")
        same(qsTrId(id, 0), other.replace("%n", "0"), id + " for 0")
        same(qsTrId(id, 11), other.replace("%n", "11"), id + " for 11")
    }

    Component.onCompleted: {
        same(qsTrId("lautta-app-name"), "Lautta", "the catalogue is installed")

        plural("lautta-dir-grid-items", "1 item", "%n items")
        plural("lautta-dir-menu-paste", "Paste 1 item", "Paste %n items")
        plural("lautta-dir-clip-cut", "1 item cut", "%n items cut")
        plural("lautta-settings-days", "1 day", "%n days")
        plural("lautta-viewers-pdf-pages", "1 page", "%n pages")
        plural("lautta-xfr-cover-count", "1 task", "%n tasks")
        plural("lautta-xfr-note-failed-body", "1 file couldn't be transferred. Tap to retry.",
               "%n files couldn't be transferred. Tap to retry.")
        plural("lautta-share-skipped", "1 file is skipped.", "%n files are skipped.")
        plural("lautta-xfr-header-paused", "1 paused", "%n paused")
        plural("lautta-dir-selected", "1 selected", "%n selected")

        same(qsTrId("lautta-xfr-title-moved-many", 1).arg("NAS"), "Moved 1 item to NAS", "a plural string with an argument")
        same(qsTrId("lautta-xfr-title-moved-many", 3).arg("NAS"), "Moved 3 items to NAS", "and its plural")
        same(qsTrId("lautta-dir-grid-items-more", 10000), "More than 10000 items", "a capped count")
    }
}
