// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/browse/RowStatus.js" as RowStatus

QtObject {
    function same(got, want, what) {
        if (got !== want)
            console.error("FAILED: " + what + ": \"" + got + "\", not \"" + want + "\"")
    }

    Component.onCompleted: {
        var cases = [
            ["account", "ready", "connected", "", "", "on"],
            ["adhoc", "ready", "connected", "", "", "on"],
            ["account", "ready", "connecting", "", "", "busy"],
            ["account", "connecting", "connecting", "", "", "busy"],
            ["account", "ready", "disconnected", "", "", "off"],
            ["adhocRecent", "offline", "disconnected", "", "", "off"],
            ["account", "ready", "failed", "Timeout", "", "failed"],
            ["account", "ready", "failed", "", "", "failed"],
            ["adhoc", "ready", "failed", "AuthFailed", "", "warning"],
            ["adhoc", "ready", "failed", "ServerIdentityChanged", "", "warning"],
            ["account", "attention", "connected", "", "", "warning"],
            ["account", "attention", "failed", "Timeout", "", "warning"],
            ["account", "ready", "", "", "", ""],
            ["account", "ready", "suspended", "", "", ""],
            ["account", "ready", undefined, undefined, undefined, ""],
            ["volume", "ready", "", "", "mmcblk1p1", "on"],
            ["volume", "unmounted", "", "", "sda1", "off"],
            ["volume", "ready", "", "", "", ""],
            ["volume", "ready", "", "", undefined, ""],
            ["volume", "unmounted", "", "", "sda1", "busy", ["sda1"]],
            ["volume", "ready", "", "", "sda1", "busy", ["sda1"]],
            ["volume", "unmounted", "", "", "sda1", "busy", ["mmcblk1p1", "sda1"]],
            ["volume", "unmounted", "", "", "sda1", "busy", ["sda1", "sda1"]],
            ["volume", "unmounted", "", "", "sda1", "off", ["mmcblk1p1"]],
            ["volume", "ready", "", "", "sda1", "on", []],
            ["volume", "ready", "", "", "sda1", "on", undefined],
            ["volume", "locked", "", "", "mmcblk1p2", "locked"],
            ["volume", "locked", "", "", "mmcblk1p2", "locked", ["mmcblk1p2"]],
            ["folder", "locked", "", "", "", ""],
            ["volume", "ready", "", "", "", "", [""]],
            ["account", "ready", "connected", "", "sda1", "on", ["sda1"]],
            ["folder", "ready", "", "", "sda1", "", ["sda1"]],
            ["folder", "ready", "connected", "", "", ""],
            ["favourite", "ready", "disconnected", "", "", ""],
            ["recents", "ready", "", "", "", ""],
            ["deleted", "ready", "", "", "", ""],
            ["nearby", "ready", "connected", "", "", ""],
            ["consent", "ready", "connected", "", "", ""],
            ["unavailable", "ready", "failed", "Timeout", "", ""]
        ]
        for (var i = 0; i < cases.length; ++i) {
            var c = cases[i]
            same(RowStatus.indicator(c[0], c[1], c[2], c[3], c[4], c[6]), c[5],
                 "dot of " + c.slice(0, 5).join("/") + (c[6] ? " busy " + c[6].join(",") : ""))
        }
    }
}
