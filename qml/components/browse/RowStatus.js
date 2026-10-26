// SPDX-License-Identifier: LGPL-2.1-or-later
// How a Browse row's connection or mount stands (SPEC §15.2), as one glowing
// dot at its right edge: "on" (connected, mounted), "busy" (connecting,
// mounting, unmounting), "off" (not connected, not mounted), "failed" (could
// not connect), "warning" (sign-in or identity), "" for nothing. No text: dot
// says it (NVB-4). Computed from row roles; no Silica here, so host tests can
// check it.
.pragma library

// A failed connection: the sign-in and identity failures are the ones the
// user can act on, as an account's attention is; any other failure says no
// more than that it failed.
function failedIndicator(error) {
    return error === "AuthFailed" || error === "ServerIdentityChanged" ? "warning" : "failed"
}

// A server (NVB-4): an attention (NVB-5) comes first. Nothing when the
// bridge tells no state (an older netvfs), rather than a state that may be
// wrong.
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

// A volume (LOC-3). Only a drive the bridge mounts can be mounted or not; any
// other volume is there because it is mounted, as device folders always are,
// and has no dot. Drive listed in `busyDrives` (Bridge.busyDrives: mount or
// unmount under way) is busy, as a server connecting is. Failure arrives as
// a notice.
function volume(status, drive, busyDrives) {
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
