// SPDX-License-Identifier: LGPL-2.1-or-later
// Texts of the Browse area that depend on engineering values from the models.
.pragma library
.import Sailfish.Silica 1.0 as Silica

function sectionTitle(section) {
    switch (section) {
    case "favourites":
        //% "Favourites"
        return qsTrId("lautta-browse-section-favourites")
    case "device":
        //% "On this device"
        return qsTrId("lautta-browse-section-device")
    case "android":
        //% "Android"
        return qsTrId("lautta-browse-section-android")
    case "volumes":
        //% "Volumes"
        return qsTrId("lautta-browse-section-volumes")
    case "servers":
        //% "Servers"
        return qsTrId("lautta-browse-section-servers")
    case "nearby":
        //% "Nearby"
        return qsTrId("lautta-browse-section-nearby")
    default:
        return ""
    }
}

// "58.2 GB free of 128 GB · exFAT" (LOC-3); empty when the space is unknown.
function volumeLine(free, total, fs) {
    if (free < 0 || total < 0)
        return fs
    //% "%1 free of %2"
    var space = qsTrId("lautta-browse-volume-space").arg(Silica.Format.formatFileSize(free)).arg(Silica.Format.formatFileSize(total))
    return fs.length > 0 ? space + " · " + fs : space
}

function attentionText(attention) {
    if (attention === "server-identity-changed")
        //% "Identity changed"
        return qsTrId("lautta-browse-attention-identity")
    //% "Sign-in failed"
    return qsTrId("lautta-browse-attention-auth")
}

// The part after the status of a server line: "SFTP · nas.home".
function serverDetail(provider, host) {
    var parts = []
    if (provider.length > 0)
        parts.push(provider)
    if (host.length > 0)
        parts.push(host)
    return parts.join(" · ")
}

function connectedLine(provider, host) {
    //% "Connected"
    var text = qsTrId("lautta-browse-server-connected")
    var detail = serverDetail(provider, host)
    return detail.length > 0 ? text + " · " + detail : text
}

function reconnectingLine(provider) {
    //% "Reconnecting…"
    var text = qsTrId("lautta-browse-server-reconnecting")
    return provider.length > 0 ? text + " · " + provider : text
}

function recentLine(provider) {
    //% "Recent"
    var text = qsTrId("lautta-browse-server-recent")
    //% "not connected"
    var off = qsTrId("lautta-browse-server-not-connected")
    return provider.length > 0 ? text + " · " + provider + " · " + off : text + " · " + off
}

function nearbyLine(provider, host) {
    return serverDetail(provider, host)
}

// Recents: "Opened · NAS › /srv/photos", "Transferred to NAS".
function recentKindLine(kind, place) {
    var what
    switch (kind) {
    case "previewed":
        //% "Previewed"
        what = qsTrId("lautta-recents-previewed")
        break
    case "transferred":
        if (place.length > 0)
            //% "Transferred to %1"
            return qsTrId("lautta-recents-transferred-to").arg(place)
        //% "Transferred"
        return qsTrId("lautta-recents-transferred")
    default:
        //% "Opened"
        what = qsTrId("lautta-recents-opened")
        break
    }
    return place.length > 0 ? what + " · " + place : what
}

function recentDayTitle(day) {
    switch (day) {
    case "today":
        //% "Today"
        return qsTrId("lautta-recents-today")
    case "yesterday":
        //% "Yesterday"
        return qsTrId("lautta-recents-yesterday")
    case "week":
        //% "Earlier this week"
        return qsTrId("lautta-recents-week")
    default:
        //% "Earlier"
        return qsTrId("lautta-recents-earlier")
    }
}

function recentFilterTitle(kind) {
    switch (kind) {
    case "opened":
        //% "Opened"
        return qsTrId("lautta-recents-opened")
    case "previewed":
        //% "Previewed"
        return qsTrId("lautta-recents-previewed")
    case "transferred":
        //% "Transferred"
        return qsTrId("lautta-recents-transferred")
    default:
        //% "Everything"
        return qsTrId("lautta-recents-everything")
    }
}

// Colours offered for favourites.
var colours = ["#e7a33c", "#7ec97a", "#e5604f", "#4fa3e5", "#9bd26a", "#b57edc", "#e57fb0", "#8a9aa6"]

function colourName(colour) {
    switch (colour) {
    case "#e7a33c":
        //% "Amber"
        return qsTrId("lautta-colour-amber")
    case "#7ec97a":
        //% "Green"
        return qsTrId("lautta-colour-green")
    case "#e5604f":
        //% "Red"
        return qsTrId("lautta-colour-red")
    case "#4fa3e5":
        //% "Blue"
        return qsTrId("lautta-colour-blue")
    case "#9bd26a":
        //% "Lime"
        return qsTrId("lautta-colour-lime")
    case "#b57edc":
        //% "Purple"
        return qsTrId("lautta-colour-purple")
    case "#e57fb0":
        //% "Pink"
        return qsTrId("lautta-colour-pink")
    default:
        //% "Grey"
        return qsTrId("lautta-colour-grey")
    }
}
