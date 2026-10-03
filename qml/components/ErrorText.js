// SPDX-License-Identifier: LGPL-2.1-or-later
// Error presentation (SPEC §20): one message per core ErrorKind name.
// context: { location, item, needed, free } (sizes in bytes; any may be absent).
.pragma library
.import Sailfish.Silica 1.0 as Silica

function _size(bytes) {
    return Silica.Format.formatFileSize(bytes)
}

function message(kind, context) {
    var c = context || {}
    var loc = c.location || ""
    var item = c.item || ""
    switch (kind) {
    case "AuthFailed":
        //% "The server didn't accept the sign-in for %1."
        return qsTrId("lautta-err-auth-failed").arg(loc)
    case "ServerIdentityChanged":
        //% "%1 is presenting a different identity than before. This can mean it was reinstalled — or that someone is intercepting the connection."
        return qsTrId("lautta-err-identity-changed").arg(loc)
    case "ServerIdentityUnknown":
        //% "%1 has not been verified yet."
        return qsTrId("lautta-err-identity-unknown").arg(loc)
    case "SecurityPolicy":
        //% "%1 doesn't meet the account's security settings."
        return qsTrId("lautta-err-security-policy").arg(loc)
    case "NoSpace":
        if (c.needed !== undefined && c.free !== undefined)
            //% "Not enough space on %1 (needs %2, %3 free)."
            return qsTrId("lautta-err-no-space-sizes").arg(loc).arg(_size(c.needed)).arg(_size(c.free))
        //% "Not enough space on %1."
        return qsTrId("lautta-err-no-space").arg(loc)
    case "PermissionDenied":
        //% "You don't have permission to change %1."
        return qsTrId("lautta-err-permission").arg(item || loc)
    case "Locked":
        //% "%1 is open elsewhere."
        return qsTrId("lautta-err-locked").arg(item)
    case "ConnectionLost":
        //% "Connection to %1 was lost. Retrying…"
        return qsTrId("lautta-err-connection-lost").arg(loc)
    case "NetworkUnreachable":
    case "TimedOut":
        //% "Can't reach %1."
        return qsTrId("lautta-err-unreachable").arg(loc)
    case "Unsupported":
        //% "%1 can't do this."
        return qsTrId("lautta-err-unsupported").arg(loc)
    case "BridgeUnavailable":
        //% "Network locations are unavailable right now."
        return qsTrId("lautta-err-bridge-gone")
    case "Sandbox":
        //% "Lautta isn't allowed to read this file."
        return qsTrId("lautta-err-sandbox")
    case "NotFound":
        //% "%1 no longer exists."
        return qsTrId("lautta-err-not-found").arg(item || loc)
    case "AlreadyExists":
        //% "An item named %1 already exists."
        return qsTrId("lautta-err-exists").arg(item)
    case "InvalidName":
        //% "This name can't be used here."
        return qsTrId("lautta-err-invalid-name")
    case "ReadOnlyFilesystem":
        //% "%1 is read-only."
        return qsTrId("lautta-err-read-only").arg(loc)
    case "DirectoryNotEmpty":
        //% "The folder isn't empty."
        return qsTrId("lautta-err-not-empty")
    case "TooManyConnections":
    case "RateLimited":
        //% "%1 is busy. Try again in a moment."
        return qsTrId("lautta-err-busy").arg(loc)
    case "Canceled":
        //% "Canceled"
        return qsTrId("lautta-err-canceled")
    default:
        //% "Something went wrong."
        return qsTrId("lautta-err-generic")
    }
}

// Actions offered with the message (§20).
function actions(kind) {
    switch (kind) {
    case "AuthFailed": return ["updateSignIn"]
    case "ServerIdentityChanged": return ["reviewInSettings"]
    case "SecurityPolicy": return ["accountSettings"]
    case "NoSpace": return ["chooseAnotherFolder"]
    case "Locked": return ["retry"]
    case "ConnectionLost": return ["retryNow"]
    case "BridgeUnavailable": return ["retry"]
    default: return []
    }
}
