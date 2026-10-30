// SPDX-License-Identifier: LGPL-2.1-or-later context: { location, item, needed, free }; sizes in
// bytes, any may be absent.
.pragma library
.import Sailfish.Silica 1.0 as Silica

function _size(bytes) {
    return Silica.Format.formatFileSize(bytes)
}

// `named` with subject filled in, `unnamed` when none: no message reads " no longer exists.".
function _pick(subject, named, unnamed) {
    return subject ? named.arg(subject) : unnamed
}

function message(kind, context) {
    var c = context || {}
    var loc = c.location || ""
    var item = c.item || ""
    switch (kind) {
    case "AuthFailed":
        return _pick(loc,
                     //% "The server didn't accept the sign-in for %1."
                     qsTrId("lautta-err-auth-failed"),
                     //% "The server didn't accept the sign-in."
                     qsTrId("lautta-err-auth-failed-unnamed"))
    case "ServerIdentityChanged":
        return _pick(loc,
                     //% "%1 is presenting a different identity than before. This can mean it was reinstalled — or that someone is intercepting the connection."
                     qsTrId("lautta-err-identity-changed"),
                     //% "The server is presenting a different identity than before. This can mean it was reinstalled — or that someone is intercepting the connection."
                     qsTrId("lautta-err-identity-changed-unnamed"))
    case "ServerIdentityUnknown":
        return _pick(loc,
                     //% "%1 has not been verified yet."
                     qsTrId("lautta-err-identity-unknown"),
                     //% "The server has not been verified yet."
                     qsTrId("lautta-err-identity-unknown-unnamed"))
    case "SecurityPolicy":
        return _pick(loc,
                     //% "%1 doesn't meet the account's security settings."
                     qsTrId("lautta-err-security-policy"),
                     //% "The server doesn't meet the account's security settings."
                     qsTrId("lautta-err-security-policy-unnamed"))
    case "NoSpace":
        return _noSpace(loc, c)
    case "PermissionDenied":
        return _pick(item || loc,
                     //% "You don't have permission to change %1."
                     qsTrId("lautta-err-permission"),
                     //% "You don't have permission to change this."
                     qsTrId("lautta-err-permission-unnamed"))
    case "Locked":
        return _pick(item,
                     //% "%1 is open elsewhere."
                     qsTrId("lautta-err-locked"),
                     //% "The item is open elsewhere."
                     qsTrId("lautta-err-locked-unnamed"))
    case "ConnectionLost":
        return _pick(loc,
                     //% "Connection to %1 was lost. Retrying…"
                     qsTrId("lautta-err-connection-lost"),
                     //% "The connection was lost. Retrying…"
                     qsTrId("lautta-err-connection-lost-unnamed"))
    case "NetworkUnreachable":
    case "TimedOut":
        return _pick(loc,
                     //% "Can't reach %1."
                     qsTrId("lautta-err-unreachable"),
                     //% "Can't reach the server."
                     qsTrId("lautta-err-unreachable-unnamed"))
    case "Unsupported":
        return _pick(loc,
                     //% "%1 can't do this."
                     qsTrId("lautta-err-unsupported"),
                     //% "This location can't do this."
                     qsTrId("lautta-err-unsupported-unnamed"))
    case "BridgeUnavailable":
        //% "Network locations are unavailable right now."
        return qsTrId("lautta-err-bridge-gone")
    case "LocationGone":
        return _pick(loc,
                     //% "%1 is no longer connected."
                     qsTrId("lautta-err-location-gone"),
                     //% "The server is no longer connected."
                     qsTrId("lautta-err-location-gone-unnamed"))
    case "Sandbox":
        //% "Lautta isn't allowed to read this file."
        return qsTrId("lautta-err-sandbox")
    case "NotFound":
        return _pick(item || loc,
                     //% "%1 no longer exists."
                     qsTrId("lautta-err-not-found"),
                     //% "The item no longer exists."
                     qsTrId("lautta-err-not-found-unnamed"))
    case "AlreadyExists":
        return _pick(item,
                     //% "An item named %1 already exists."
                     qsTrId("lautta-err-exists"),
                     //% "An item with this name already exists."
                     qsTrId("lautta-err-exists-unnamed"))
    case "InvalidName":
        //% "This name can't be used here."
        return qsTrId("lautta-err-invalid-name")
    case "ReadOnlyFilesystem":
        return _pick(loc,
                     //% "%1 is read-only."
                     qsTrId("lautta-err-read-only"),
                     //% "This location is read-only."
                     qsTrId("lautta-err-read-only-unnamed"))
    case "DirectoryNotEmpty":
        //% "The folder isn't empty."
        return qsTrId("lautta-err-not-empty")
    case "TooManyConnections":
    case "RateLimited":
        return _pick(loc,
                     //% "%1 is busy. Try again in a moment."
                     qsTrId("lautta-err-busy"),
                     //% "The server is busy. Try again in a moment."
                     qsTrId("lautta-err-busy-unnamed"))
    case "Canceled":
        //% "Canceled"
        return qsTrId("lautta-err-canceled")
    default:
        //% "Something went wrong."
        return qsTrId("lautta-err-generic")
    }
}

function _noSpace(loc, c) {
    if (c.needed !== undefined && c.free !== undefined) {
        if (loc)
            //% "Not enough space on %1 (needs %2, %3 free)."
            return qsTrId("lautta-err-no-space-sizes").arg(loc).arg(_size(c.needed)).arg(_size(c.free))
        //% "Not enough space (needs %1, %2 free)."
        return qsTrId("lautta-err-no-space-sizes-unnamed").arg(_size(c.needed)).arg(_size(c.free))
    }
    return _pick(loc,
                 //% "Not enough space on %1."
                 qsTrId("lautta-err-no-space"),
                 //% "Not enough space."
                 qsTrId("lautta-err-no-space-unnamed"))
}

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
