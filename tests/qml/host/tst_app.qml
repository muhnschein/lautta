// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the App singleton (no Silica): URI helpers, settings,
// clipboard and viewer choice behave as doc/QML-API.md says.
import QtQuick 2.6
import Lautta 1.0

QtObject {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    Component.onCompleted: {
        check(App.ready, "core opened")
        var docs = "lautta://user-documents/"
        var child = App.childUri(docs, "a b.txt")
        check(child === "lautta://user-documents/a%20b.txt", "childUri encodes: " + child)
        check(App.parentUri(child) === docs, "parentUri")
        check(App.nameOf(child) === "a b.txt", "nameOf")
        check(App.childUri(docs, "x/y") === "", "names with a slash are refused")
        check(App.categoryOf("photo.JPG") === "image", "category by extension")
        check(App.viewerFor("notes.md", "") === "markdown", "markdown viewer")
        check(App.viewerFor("song.flac", "") === "audio", "audio viewer")
        check(App.viewerFor("report.pdf", "") === "external", "pdf goes to Open with")

        check(App.setting("remorse_seconds") === 5, "default remorse")
        check(App.setSetting("remorse_seconds", JSON.stringify(8)), "setSetting accepts")
        check(App.setting("remorse_seconds") === 8, "setting updated")
        check(JSON.parse(App.settingsJson).remorse_seconds === 8, "settingsJson updated")
        check(!App.setSetting("no_such_key", "1"), "unknown keys are refused")

        check(App.clipboardCount === 0, "empty clipboard")
        check(App.cut(JSON.stringify([child])), "cut")
        check(App.clipboardCount === 1 && App.clipboardCut, "cut recorded")
        check(App.canPasteInto(docs), "paste into the parent")
        var taken = JSON.parse(App.takeClipboard())
        check(taken.cut && taken.items[0] === child, "take returns the items")
        check(App.clipboardCount === 0, "a cut is consumed by paste")
        check(!App.cut("not json"), "bad input is refused")
    }
}
