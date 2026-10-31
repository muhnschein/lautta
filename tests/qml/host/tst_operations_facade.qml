// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0

QtObject {
    id: test

    property var failures: []

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function lastFailure() {
        return failures.length > 0 ? failures[failures.length - 1] : ""
    }

    property TrashModel trash: TrashModel { }

    Component.onCompleted: {
        Operations.failed.connect(function(kind, message) {
            failures.push(kind)
        })

        check(Operations.jobsActive === 0, "no jobs at start")

        Operations.copyTo("not json", "lautta://user-documents/")
        check(lastFailure() === "InvalidArgument", "copyTo refuses bad uris: " + lastFailure())
        Operations.moveTo(JSON.stringify(["lautta://user-documents/a"]), "nope")
        check(failures.length === 2 && lastFailure() === "InvalidArgument", "moveTo refuses a bad destination")
        Operations.remove("{}")
        check(failures.length === 3, "remove refuses bad uris")
        Operations.compress(JSON.stringify(["lautta://user-documents/a"]), "lautta://user-downloads/", "x", "rar")
        check(failures.length === 4 && lastFailure() === "InvalidArgument", "compress refuses unknown formats")
        check(Operations.jobsActive === 0, "a refused compress starts no job")
        Operations.extractHere("nope")
        check(failures.length === 5 && lastFailure() === "InvalidArgument", "extractHere refuses bad uris")
        Operations.openArchive("nope")
        check(failures.length === 6, "openArchive refuses bad uris")

        Operations.resolveConflict(999, 0, "KeepBoth", false)
        check(lastFailure() === "NotFound", "unknown plan has no conflicts to answer")
        Operations.resolveConflict(999, 0, "Smash", false)
        check(lastFailure() === "InvalidArgument", "unknown choices are refused")
        Operations.discardPlan(999)
        check(Operations.conflictsJson(999) === "[]", "no conflicts for an unknown plan")
        Operations.cancelJob(42)

        check(!Operations.isArchive("lautta://user-documents/"), "a user folder is not an archive")
        check(Operations.archiveSource("lautta://user-documents/") === "", "no archive source for a folder")
        // Registry knows user folders only if test home is visible (not under SDK sb2).
        var known = App.locationName("lautta://user-documents/") === "Documents"
        var shown = Operations.displayPath("lautta://user-documents/Uni/x.txt")
        check(shown.indexOf(" \u203a Uni \u203a x.txt") > 0, "display path: " + shown)
        check(!known || shown === "Documents \u203a Uni \u203a x.txt", "display path with a known location: " + shown)
        check(Operations.displayPath("junk") === "", "display path of junk")

        var dest = JSON.parse(Operations.destinationsJson())
        var names = dest.map(function(d) { return d.name })
        check(!known || (names.indexOf("Documents") >= 0 && names.indexOf("Downloads") >= 0), "destinations: " + names)
        check(!known || (dest[0].kind === "device" && dest[0].uri.indexOf("lautta://") === 0), "destination shape")

        var probed = JSON.parse(Operations.probeShared(JSON.stringify([
            "file:///nonexistent/shared%20file.jpg", "/also/missing.txt"])))
        check(probed.length === 2 && !probed[0].readable && !probed[1].readable, "unreadable shares are flagged")
        check(probed[0].name === "shared file.jpg", "file URLs are decoded: " + probed[0].name)
        check(JSON.parse(Operations.probeShared("junk")).length === 0, "junk paths probe to nothing")

        check(trash.count === 0 && trash.totalSize === 0 && !trash.loaded && !trash.busy, "empty trash model")
    }
}
