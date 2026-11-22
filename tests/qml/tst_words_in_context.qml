// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/dialogs"
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
        FolderPickerDialog {
            id: picker
            title: "Copy to"
            acceptText: "Copy here"
            startUri: "lautta://user-documents/"
            Component.onCompleted: {
                test.checkPermission()
                test.checkPasswords()
                test.checkItemStates()
                test.checkCanceledItems()
                test.checkPending()
                test.checkLevels()
                test.checkPicker(picker)
                test.reachedEnd = true
            }
        }
    }
}
