// SPDX-License-Identifier: LGPL-2.1-or-later
// The status line of a server row in Browse (SPEC §15.2): only how its
// connection stands (NVB-4), from the row's status, attention, connection
// and connectionError roles. No Silica here, so that the host tests can
// check it.
.pragma library

function attentionText(attention) {
    if (attention === "server-identity-changed")
        //% "Identity changed"
        return qsTrId("lautta-browse-attention-identity")
    //% "Sign-in failed"
    return qsTrId("lautta-browse-attention-auth")
}

// A failed connection: the sign-in and identity failures say what the
// attention of an account would say, any other failure says no more.
function failedText(error) {
    if (error === "AuthFailed")
        return attentionText("auth-failed")
    if (error === "ServerIdentityChanged")
        return attentionText("server-identity-changed")
    //% "Couldn't connect"
    return qsTrId("lautta-browse-server-failed")
}

// "Connected", "Connecting…", "Not connected" or "Couldn't connect"; an
// attention (NVB-5) comes first. Nothing when the bridge tells no state
// (an older netvfs), rather than a state that may be wrong.
function text(status, attention, connection, connectionError) {
    if (status === "attention")
        return attentionText(attention)
    switch (connection) {
    case "connected":
        //% "Connected"
        return qsTrId("lautta-browse-server-connected")
    case "connecting":
        //% "Connecting…"
        return qsTrId("lautta-browse-server-connecting")
    case "disconnected":
        //% "Not connected"
        return qsTrId("lautta-browse-server-disconnected")
    case "failed":
        return failedText(connectionError)
    default:
        return ""
    }
}

// What stands before the text: "warning" for an attention, "busy" while
// connecting, a dot for "connected", "failed" and "disconnected" (each its
// own), "" for nothing.
function indicator(status, connection) {
    if (status === "attention")
        return "warning"
    switch (connection) {
    case "connecting":
        return "busy"
    case "connected":
    case "failed":
    case "disconnected":
        return connection
    default:
        return ""
    }
}
