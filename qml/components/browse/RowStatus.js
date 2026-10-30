// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function failedIndicator(error) {
    return error === "AuthFailed" || error === "ServerIdentityChanged" ? "warning" : "failed"
}

function server(status, connection, connectionError) {
    if (status === "attention")
        return "warning"
    switch (connection) {
    case "connecting":
        return "busy"
    case "connected":
        return "on"
    case "disconnected":
        return "off"
    case "failed":
        return failedIndicator(connectionError)
    default:
        return ""
    }
}

function volume(status, drive, busyDrives) {
    if (status === "locked")
        return "locked"
    if (drive.length > 0 && busyDrives && busyDrives.indexOf(drive) >= 0)
        return "busy"
    if (status === "unmounted")
        return "off"
    return drive.length > 0 ? "on" : ""
}

function indicator(kind, status, connection, connectionError, drive, busyDrives) {
    switch (kind) {
    case "account":
    case "adhoc":
    case "adhocRecent":
        return server(status, connection, connectionError)
    case "volume":
        return volume(status, drive || "", busyDrives)
    default:
        return ""
    }
}
