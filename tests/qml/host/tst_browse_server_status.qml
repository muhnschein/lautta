// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the status line of Browse's server rows (SPEC §15.2, NVB-4,
// NVB-12): it tells only how the connection stands, an attention first,
// and nothing when the bridge tells no state. On the host qsTrId gives back
// the id.
import QtQuick 2.6
import "../../../qml/components/browse/ServerStatus.js" as ServerStatus

QtObject {
    function same(got, want, what) {
        if (got !== want)
            console.error("FAILED: " + what + ": " + got + ", not " + want)
    }

    Component.onCompleted: {
        // [status, attention, connection, connectionError, text, indicator]
        var cases = [
            ["ready", "", "connected", "", "lautta-browse-server-connected", "connected"],
            ["ready", "", "connecting", "", "lautta-browse-server-connecting", "busy"],
            // The bridge itself reconnecting: the core says connecting.
            ["connecting", "", "connecting", "", "lautta-browse-server-connecting", "busy"],
            ["ready", "", "disconnected", "", "lautta-browse-server-disconnected", "disconnected"],
            // A recent ad-hoc server is not connected.
            ["offline", "", "disconnected", "", "lautta-browse-server-disconnected", "disconnected"],
            ["ready", "", "failed", "Timeout", "lautta-browse-server-failed", "failed"],
            ["ready", "", "failed", "", "lautta-browse-server-failed", "failed"],
            ["ready", "", "failed", "AuthFailed", "lautta-browse-attention-auth", "failed"],
            ["ready", "", "failed", "ServerIdentityChanged", "lautta-browse-attention-identity", "failed"],
            // An attention wins over the state.
            ["attention", "auth-failed", "connected", "", "lautta-browse-attention-auth", "warning"],
            ["attention", "server-identity-changed", "failed", "Timeout", "lautta-browse-attention-identity", "warning"],
            // An older bridge tells no state: nothing rather than "Connected".
            ["ready", "", "", "", "", ""],
            ["ready", "", "suspended", "", "", ""],
            ["ready", "", undefined, undefined, "", ""]
        ]
        for (var i = 0; i < cases.length; ++i) {
            var c = cases[i]
            var what = c.slice(0, 4).join("/")
            same(ServerStatus.text(c[0], c[1], c[2], c[3]), c[4], "text of " + what)
            same(ServerStatus.indicator(c[0], c[2]), c[5], "indicator of " + what)
        }
    }
}
