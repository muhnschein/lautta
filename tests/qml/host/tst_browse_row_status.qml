// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the status dot of Browse's rows (SPEC §15.2, NVB-4, NVB-5,
// NVB-12, LOC-3): a server's connection and a drive's mount or unmount under
// way, an attention first, a padlock for a locked drive, and no dot when the
// state is not told, for a volume the bridge does not mount and for every
// other row.
import QtQuick 2.6
import "../../../qml/components/browse/RowStatus.js" as RowStatus

QtObject {
    function same(got, want, what) {
        if (got !== want)
            console.error("FAILED: " + what + ": \"" + got + "\", not \"" + want + "\"")
    }

    Component.onCompleted: {
        // [kind, status, connection, connectionError, drive, icon, busyDrives]
        var cases = [
            ["account", "ready", "connected", "", "", "on"],
            ["adhoc", "ready", "connected", "", "", "on"],
            ["account", "ready", "connecting", "", "", "busy"],
            // The bridge itself reconnecting: the core says connecting.
            ["account", "connecting", "connecting", "", "", "busy"],
            ["account", "ready", "disconnected", "", "", "off"],
            // A recent ad-hoc server is not connected.
            ["adhocRecent", "offline", "disconnected", "", "", "off"],
            ["account", "ready", "failed", "Timeout", "", "failed"],
            ["account", "ready", "failed", "", "", "failed"],
            // What the user can act on is a warning, as an attention is.
            ["adhoc", "ready", "failed", "AuthFailed", "", "warning"],
            ["adhoc", "ready", "failed", "ServerIdentityChanged", "", "warning"],
            // An attention wins over the state.
            ["account", "attention", "connected", "", "", "warning"],
            ["account", "attention", "failed", "Timeout", "", "warning"],
            // An older bridge tells no state: no icon rather than a wrong one.
            ["account", "ready", "", "", "", ""],
            ["account", "ready", "suspended", "", "", ""],
            ["account", "ready", undefined, undefined, undefined, ""],
            // Volumes: a drive the bridge mounts is mounted or not; any other
            // volume is there because it is mounted, like internal storage.
            ["volume", "ready", "", "", "mmcblk1p1", "on"],
            ["volume", "unmounted", "", "", "sda1", "off"],
            ["volume", "ready", "", "", "", ""],
            ["volume", "ready", "", "", undefined, ""],
            // A drive with a mount or unmount under way is busy, as a server
            // connecting is, whichever way it stands; others stay as they are.
            ["volume", "unmounted", "", "", "sda1", "busy", ["sda1"]],
            ["volume", "ready", "", "", "sda1", "busy", ["sda1"]],
            ["volume", "unmounted", "", "", "sda1", "busy", ["mmcblk1p1", "sda1"]],
            ["volume", "unmounted", "", "", "sda1", "busy", ["sda1", "sda1"]],
            ["volume", "unmounted", "", "", "sda1", "off", ["mmcblk1p1"]],
            ["volume", "ready", "", "", "sda1", "on", []],
            ["volume", "ready", "", "", "sda1", "on", undefined],
            // A drive Settings › Storage unlocks first: a padlock, never busy
            // (the app does not mount it).
            ["volume", "locked", "", "", "mmcblk1p2", "locked"],
            ["volume", "locked", "", "", "mmcblk1p2", "locked", ["mmcblk1p2"]],
            ["folder", "locked", "", "", "", ""],
            // A volume without a drive is never busy, and neither is any other row.
            ["volume", "ready", "", "", "", "", [""]],
            ["account", "ready", "connected", "", "sda1", "on", ["sda1"]],
            ["folder", "ready", "", "", "sda1", "", ["sda1"]],
            // Every other row has none, whatever its roles hold.
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
