// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the texts of the settings pages (SPEC §18): the line under
// each entry of Settings tells what is chosen on its page, and the day and
// size choices have their labels. On the host qsTrId gives back the id.
import QtQuick 2.6
import "../../../qml/components/search/SettingsText.js" as SettingsText

QtObject {
    function same(got, want, what) {
        if (JSON.stringify(got) !== JSON.stringify(want))
            console.error("FAILED: " + what + ": " + JSON.stringify(got) + ", not " + JSON.stringify(want))
    }

    Component.onCompleted: {
        same(SettingsText.viewName("list"), "lautta-settings-summary-list", "a list")
        same(SettingsText.viewName("grid"), "lautta-settings-summary-grid", "a grid")
        same(SettingsText.sortName("name"), "lautta-settings-summary-by-name", "by name")
        same(SettingsText.sortName("size"), "lautta-settings-summary-by-size", "by size")
        same(SettingsText.sortName("modified"), "lautta-settings-summary-by-modified", "by date")
        same(SettingsText.sortName("type"), "lautta-settings-summary-by-type", "by type")
        same(SettingsText.sortName(undefined), "lautta-settings-summary-by-name", "names when the key is unknown")

        same(SettingsText.deleting(true, 30), "lautta-settings-summary-trash", "deletes go to Trash")
        same(SettingsText.deleting(false, 30), "lautta-settings-summary-no-trash", "deletes are permanent")
        same(SettingsText.recents(true), "lautta-settings-summary-recents-on", "recents on")
        same(SettingsText.recents(false), "lautta-settings-summary-recents-off", "recents off")
        same(SettingsText.transfers(2), "lautta-settings-summary-transfers", "transfers at once")
        same(SettingsText.search(8), "lautta-settings-summary-search", "search depth")

        same(SettingsText.days([7, 30]), ["lautta-settings-days", "lautta-settings-days"], "a label for each day count")
        same(SettingsText.megabytes([5, 20]), ["lautta-settings-up-to-mb", "lautta-settings-up-to-mb"],
             "a label for each size")
    }
}
