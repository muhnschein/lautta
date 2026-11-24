// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/search/SettingsText.js" as SettingsText

QtObject {
    property bool reachedEnd
    function same(got, want, what) {
        if (JSON.stringify(got) !== JSON.stringify(want))
            console.error("FAILED: " + what + ": " + JSON.stringify(got) + ", not " + JSON.stringify(want))
    }

    Component.onCompleted: {
        same(SettingsText.days([7, 30]), ["lautta-settings-days", "lautta-settings-days"], "a label for each day count")
        same(SettingsText.megabytes([5, 20]), ["lautta-settings-up-to-mb", "lautta-settings-up-to-mb"],
             "a label for each size")
        same([100000000, 1000000000, 10000000000, 1500000000].map(function (b) { return SettingsText.largeOperation(10, b) }),
             ["lautta-settings-large-mb", "lautta-settings-large-gb", "lautta-settings-large-gb", "lautta-settings-large-gb"],
             "large operations in MB below a GB, else in GB")
        reachedEnd = true
    }
}
