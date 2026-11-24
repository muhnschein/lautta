// SPDX-License-Identifier: LGPL-2.1-or-later Server problems the user must fix in Settings, shown as
// warning on SCR-51 and in folder placeholder. Identity change = security level.
.pragma library
.import "ErrorText.js" as ErrorText

function kindOf(attention) {
    switch (attention) {
    case "server-identity-changed": return "ServerIdentityChanged"
    case "auth-failed": return "AuthFailed"
    default: return ""
    }
}

function level(kind) {
    switch (kind) {
    case "ServerIdentityChanged": return "security"
    case "AuthFailed": return "warning"
    default: return ""
    }
}

function title(kind) {
    switch (kind) {
    case "ServerIdentityChanged":
        //% "Server identity changed"
        return qsTrId("lautta-attention-identity-title")
    case "AuthFailed":
        //% "Sign-in failed"
        return qsTrId("lautta-attention-auth-title")
    default: return ""
    }
}

function hint(kind, location) {
    var text = ErrorText.message(kind, { "location": location })
    if (kind === "ServerIdentityChanged")
        //% "Lautta won't connect until you check the new identity."
        text += "\n\n" + qsTrId("lautta-attention-identity-blocked")
    return text
}

function fixText(kind) {
    switch (kind) {
    case "ServerIdentityChanged":
        //% "Review identity"
        return qsTrId("lautta-attention-review")
    case "AuthFailed":
        //% "Update sign-in"
        return qsTrId("lautta-attention-update")
    default: return ""
    }
}
