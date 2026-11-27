// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/pages"
import "../../qml/components/ErrorText.js" as ErrorText
import "../../qml/components/transfers/TransferText.js" as TransferText
import "../../qml/components/Access.js" as Access

ApplicationWindow {
    id: test
    property bool reachedEnd

    function same(got, want, what) {
        if (got !== want)
            console.error("FAILED: " + what + ": \"" + got + "\", not \"" + want + "\"")
    }

    function checkPermission() {
        same(ErrorText.message("PermissionDenied", { "item": "a.mp4", "open": true }),
             qsTrId("lautta-err-permission-open").arg("a.mp4"), "reading says open")
        same(ErrorText.message("PermissionDenied", { "open": true }),
             qsTrId("lautta-err-permission-open-unnamed"), "reading unnamed says open")
        same(ErrorText.message("PermissionDenied", { "location": "Documents", "open": true }),
             qsTrId("lautta-err-permission-open").arg("Documents"), "reading a folder names its location")
        same(ErrorText.message("PermissionDenied", { "item": "a.txt" }),
             qsTrId("lautta-err-permission").arg("a.txt"), "change says change")
        same(ErrorText.message("PermissionDenied", {}),
             qsTrId("lautta-err-permission-unnamed"), "change unnamed says change")
    }

    function checkPasswords() {
        same(ErrorText.message("PasswordRequired", { "item": "a.zip" }),
             qsTrId("lautta-err-password-required").arg("a.zip"), "locked names the file")
        same(ErrorText.message("PasswordRequired", {}),
             qsTrId("lautta-err-password-required-unnamed"), "locked unnamed")
        same(ErrorText.message("WrongPassword", { "item": "a.7z" }),
             qsTrId("lautta-err-wrong-password").arg("a.7z"), "wrong password names the file")
        same(ErrorText.message("WrongPassword", {}),
             qsTrId("lautta-err-wrong-password-unnamed"), "wrong password unnamed")
    }

    function checkNoSound() {
        same(ErrorText.message("NoSound", { "item": "a.mkv", "location": "Videos", "open": true }),
             qsTrId("lautta-err-no-sound"), "no file name, the header shows it")
    }

    function checkItemStates() {
        var cases = [
            ["copy", "", "lautta-xfr-item-copying"],
            ["move", "", "lautta-xfr-item-moving"],
            ["delete", "", "lautta-xfr-item-deleting"],
            ["compress", "", "lautta-xfr-item-compressing"],
            ["extract", "", "lautta-xfr-item-extracting"],
            ["move", "upload", "lautta-xfr-item-uploading"],
            ["copy", "download", "lautta-xfr-item-downloading"],
            ["", "", "lautta-xfr-item-copying"]
        ]
        for (var i = 0; i < cases.length; ++i) {
            var c = cases[i]
            same(TransferText.itemStatus("running", "dir", 0, c[1], c[0]), qsTrId(c[2]),
                 "running item of " + c[0] + " " + c[1])
        }
    }

    function checkCanceledItems() {
        var shown = TransferText.itemState("pending", "canceled")
        same(TransferText.itemStatus(shown, "file", 0, "", "copy"), qsTrId("lautta-xfr-canceled"),
             "unfinished item of cancelled task")
        same(TransferText.itemIcon(shown), "image://theme/icon-m-dismiss", "cancelled item icon")
        same(TransferText.itemState("pending", "paused"), "pending", "paused task keeps queued items")
        same(TransferText.itemState("done", "canceled"), "done", "done item of cancelled task stays done")
    }

    function checkPending() {
        same(TransferText.pendingBody(true), qsTrId("lautta-xfr-note-pending-body"), "resume at start")
        same(TransferText.pendingBody(false), qsTrId("lautta-xfr-note-pending-manual-body"), "no resume at start")
        same(TransferText.pendingCover(2, true), qsTrId("lautta-xfr-cover-pending", 2), "cover, resume at start")
        same(TransferText.pendingCover(2, false), qsTrId("lautta-xfr-cover-stopped", 2), "cover, no resume at start")
    }

    function checkLevels() {
        same(Access.name("read"), qsTrId("lautta-access-read"), "Read only")
        same(Access.name("read-add"), qsTrId("lautta-access-read-add"), "Read and add")
        same(Access.name("add"), qsTrId("lautta-access-add"), "Add only")
        same(Access.name("full"), "", "full access has no notice")
        same(Access.name(""), "", "no level, no notice")
        var failed = { "state": "failed", "itemsFailed": 1, "itemsDone": 0, "refusedLevel": "read",
                       "refusedName": "NAS" }
        same(TransferText.barDetail("failed", failed, 0, 0), qsTrId("lautta-xfr-refused-read").arg("NAS"),
             "stopped by a narrower level says what is left")
        failed.refusedLevel = "read-add"
        same(TransferText.barDetail("failed", failed, 0, 0), qsTrId("lautta-xfr-refused-read-add").arg("NAS"),
             "read and add")
        failed.refusedLevel = "add"
        same(TransferText.barDetail("failed", failed, 0, 0), qsTrId("lautta-xfr-refused-add").arg("NAS"),
             "add only")
        failed.refusedLevel = ""
        same(TransferText.barDetail("failed", failed, 0, 0), qsTrId("lautta-xfr-failed"), "any other failure")
    }

    function checkActivityRows() {
        var row = function (more) {
            var r = { "group": "active", "dateGroup": "inProgress", "kind": "copy", "state": "running",
                      "title": "a.txt", "direction": "local", "waitReason": "", "bytesDone": 1048576,
                      "bytesTotal": 4194304, "rate": 1024, "eta": 240, "itemsDone": 0, "itemsTotal": 1,
                      "itemsFailed": 0, "destName": "Documents", "finishedMs": -1, "questions": 0 }
            for (var key in more)
                r[key] = more[key]
            return r
        }
        var progress = qsTrId("lautta-xfr-progress").arg(TransferText.size(1048576)).arg(TransferText.size(4194304))
        same(TransferText.rowLine(row({}), false),
             progress + " · " + qsTrId("lautta-xfr-eta-left").arg(TransferText.duration(240)),
             "a running task says how far and how long, not how fast")
        same(TransferText.rowLine(row({ "rate": 0 }), false), progress, "no time left without a rate")
        same(TransferText.rowLine(row({ "state": "queued", "bytesDone": 0 }), false), qsTrId("lautta-xfr-queued"),
             "a queued task says so")
        same(TransferText.rowLine(row({ "group": "waiting", "state": "waiting", "waitReason": "question", "questions": 2 }), false),
             qsTrId("lautta-xfr-needs-answer", 2), "a task with questions asks for an answer")
        same(TransferText.rowLine(row({ "group": "waiting", "state": "waiting", "waitReason": "network" }), false),
             qsTrId("lautta-xfr-wait-network"), "a waiting task says what it waits for")
        var ended = { "group": "history", "dateGroup": "today", "finishedMs": new Date(2026, 10, 27, 18, 2).getTime() }
        same(TransferText.rowLine(row(ended), false), "", "a task that went well says nothing more")
        ended.state = "canceled"
        same(TransferText.rowLine(row(ended), false), qsTrId("lautta-xfr-canceled"), "a cancelled task says so")
        ended.state = "failed"
        ended.itemsDone = 3
        ended.itemsFailed = 2
        ended.itemsTotal = 5
        same(TransferText.rowLine(row(ended), false), qsTrId("lautta-xfr-failed-of", 2).arg(5), "partly failed: how many of all")
        same(TransferText.stateGlyph(row(ended)), "image://theme/icon-s-warning", "a failure has the warning glyph")
        same(TransferText.rowWarns(row(ended)), true, "a failure is in the warning colour")
        ended.itemsDone = 0
        same(TransferText.rowLine(row(ended), false), qsTrId("lautta-xfr-failed"), "nothing done: failed")
        same(TransferText.rowTime(row(ended)), Format.formatDate(new Date(ended.finishedMs), Formatter.TimeValue),
             "a task from today shows its time")
        ended.dateGroup = "yesterday"
        same(TransferText.rowTime(row(ended)), Format.formatDate(new Date(ended.finishedMs), Formatter.TimeValue),
             "a task from yesterday shows its time, under its section")
        ended.dateGroup = "last7Days"
        same(TransferText.rowTime(row(ended)), Format.formatDate(new Date(ended.finishedMs), Formatter.DateMediumWithoutYear),
             "an older task shows its day")
        ended.dateGroup = "older"
        same(TransferText.rowTime(row(ended)), Format.formatDate(new Date(ended.finishedMs), Formatter.DateMedium),
             "a much older task shows its year")
        same(TransferText.rowTime(row({})), "", "a running task has no time")
        same(TransferText.stateGlyph(row({})), "", "a running task has no glyph")
        same(TransferText.rowWarns(row({})), false, "a running task is not a warning")
        same(TransferText.stateGlyph(row({ "group": "waiting", "state": "waiting", "waitReason": "network" })),
             "image://theme/icon-s-time", "a waiting task has the clock")
        same(TransferText.stateGlyph(row({ "group": "paused", "state": "paused" })), "image://theme/icon-m-pause",
             "a paused task has the pause glyph")
        same(TransferText.stateGlyph(row({ "group": "waiting", "state": "waiting", "waitReason": "question", "questions": 1 })),
             "image://theme/icon-s-warning", "a task with questions has the warning glyph")
        var parts = function (more) {
            var p = TransferText.titleParts(row(more))
            return p.before + p.rest
        }
        same(parts({}), qsTrId("lautta-xfr-title-copying-one").arg("a.txt").arg("Documents"), "a running task is doing it")
        same(parts({ "state": "queued" }), qsTrId("lautta-xfr-title-copy-one").arg("a.txt").arg("Documents"),
             "a queued task is still to do")
        same(parts({ "group": "history", "state": "completed" }),
             qsTrId("lautta-xfr-title-copied-one").arg("a.txt").arg("Documents"), "a finished task is done")
    }

    function checkPicker(picker) {
        same(picker.rootStatus("attention", "auth-failed", "NAS"), qsTrId("lautta-attention-auth-title"),
             "sign-in failure is words, not its code")
        same(picker.rootStatus("attention", "server-identity-changed", "NAS"),
             qsTrId("lautta-attention-identity-title"), "changed identity is words, not its code")
        same(picker.rootStatus("offline", "", "NAS"), qsTrId("lautta-err-unreachable").arg("NAS"),
             "offline server can't be reached")
        same(picker.rootStatus("connecting", "", "NAS"), "", "connecting server is not called unreachable")
        same(picker.rootStatus("ready", "", "NAS"), "", "ready server has no line")
    }

    initialPage: Component {
        ShareTargetPage {
            id: picker
            Component.onCompleted: {
                test.checkPermission()
                test.checkPasswords()
                test.checkNoSound()
                test.checkItemStates()
                test.checkCanceledItems()
                test.checkPending()
                test.checkLevels()
                test.checkActivityRows()
                test.checkPicker(picker)
                test.reachedEnd = true
            }
        }
    }
}
