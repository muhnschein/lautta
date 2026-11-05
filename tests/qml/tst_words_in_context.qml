// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/dialogs"
import "../../qml/components/ErrorText.js" as ErrorText
import "../../qml/components/transfers/TransferText.js" as TransferText

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

    function checkPending() {
        same(TransferText.pendingBody(true), qsTrId("lautta-xfr-note-pending-body"), "resume at start")
        same(TransferText.pendingBody(false), qsTrId("lautta-xfr-note-pending-manual-body"), "no resume at start")
        same(TransferText.pendingCover(2, true), qsTrId("lautta-xfr-cover-pending", 2), "cover, resume at start")
        same(TransferText.pendingCover(2, false), qsTrId("lautta-xfr-cover-stopped", 2), "cover, no resume at start")
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
                test.checkItemStates()
                test.checkPending()
                test.checkPicker(picker)
                test.reachedEnd = true
            }
        }
    }
}
