// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0

QtObject {
    property bool reachedEnd
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    Component.onCompleted: {
        check(App.ready, "core opened")
        check(Qt.fontFamilies().indexOf(App.monoFont) >= 0, "bundled font loaded: " + App.monoFont)
        var docs = "lautta://user-documents/"
        var child = App.childUri(docs, "a b.txt")
        check(child === "lautta://user-documents/a%20b.txt", "childUri encodes: " + child)
        check(App.parentUri(child) === docs, "parentUri")
        check(App.nameOf(child) === "a b.txt", "nameOf")
        check(App.childUri(docs, "x/y") === "", "names with a slash are refused")
        check(App.isRemote("lautta://nv-account:1/srv/photos"), "an account is remote")
        check(!App.isRemote(child), "a local folder is not remote")
        check(!App.isRemote("lautta://arc-1/a.txt"), "an archive is not remote")
        check(!App.isRemote("") && !App.isRemote("nv-account:1"), "nothing is remote without a URI")
        check(App.canAddTo(docs), "files can be added to Documents")
        check(!App.canAddTo("lautta://arc-1/"), "nothing can be added where no place is")
        check(!App.canAddTo("junk"), "nothing can be added without a URI")
        check(App.categoryOf("photo.JPG") === "image", "category by extension")
        check(App.viewerFor("notes.md", "") === "markdown", "markdown viewer")
        check(App.viewerFor("notes.mkd", "") === "markdown", "other Markdown names too")
        check(App.viewerFor("lautta://user-documents/notes.txt", "") === "prose", "plain text reads as prose")
        check(App.viewerFor("app.log", "") === "text", "logs stay in the text viewer")
        check(App.viewerFor("song.flac", "") === "audio", "audio viewer")
        check(App.viewerFor("report.pdf", "") === "pdf", "pdf has its own viewer")
        check(App.viewerFor("report.docx", "") === "external", "office documents go to Open with")
        check(App.viewerFor("app.ts", "") === "text", ".ts without a video type is TypeScript")
        check(App.viewerFor("rec.ts", "video/mp2t") === "video", ".ts with a video type is MPEG-TS")
        check(App.viewerFor("clip.m2ts", "") === "video", "M2TS is video")

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
        reachedEnd = true
    }
}
