// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the Operations singleton and TrashModel (no Silica): what can
// be decided without waiting for the transfer engine. End to end runs are
// covered by crates/lautta-core/tests/app_operations.rs.
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

        // Bad arguments are refused at once, with the engineering kind.
        Operations.copyTo("not json", "lautta://user-documents/")
        check(lastFailure() === "InvalidArgument", "copyTo refuses bad uris: " + lastFailure())
        Operations.moveTo(JSON.stringify(["lautta://user-documents/a"]), "nope")
        check(failures.length === 2 && lastFailure() === "InvalidArgument", "moveTo refuses a bad destination")
        Operations.remove("{}")
        check(failures.length === 3, "remove refuses bad uris")
        Operations.compress(JSON.stringify(["lautta://user-documents/a"]), "lautta://user-downloads/", "x", "rar")
        check(failures.length === 4 && lastFailure() === "InvalidArgument", "compress refuses unknown formats")
        check(Operations.jobsActive === 0, "a refused compress starts no job")
        Operations.extract("nope", "lautta://user-downloads/")
        check(failures.length === 5, "extract refuses bad uris")
        Operations.openArchive("nope")
        check(failures.length === 6, "openArchive refuses bad uris")
        Operations.measure("[1]")
        check(failures.length === 7, "measure refuses bad uris")

        // Plans that do not exist.
        Operations.startPlan(999)
        check(lastFailure() === "NotFound" && failures.length === 8, "unknown plan cannot start")
        Operations.resolveConflict(999, 0, "KeepBoth", false)
        check(lastFailure() === "NotFound", "unknown plan has no conflicts to answer")
        Operations.resolveConflict(999, 0, "Smash", false)
        check(lastFailure() === "InvalidArgument", "unknown choices are refused")
        Operations.discardPlan(999)
        check(Operations.conflictsJson(999) === "[]", "no conflicts for an unknown plan")
        Operations.cancelJob(42)

        // Helpers for the pages.
        check(!Operations.isArchive("lautta://user-documents/"), "a user folder is not an archive")
        check(Operations.archiveSource("lautta://user-documents/") === "", "no archive source for a folder")
        // The registry only knows the user folders when the test home is
        // visible to the process (not under the SDK's sb2 sandbox).
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

        // Recently deleted starts empty and unloaded.
        check(trash.count === 0 && trash.totalSize === 0 && !trash.loaded && !trash.busy, "empty trash model")
    }
}
