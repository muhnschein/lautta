// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library
.import Sailfish.Silica 1.0 as Silica

function sectionTitle(section) {
    switch (section) {
    case "recents":
        //% "Recents"
        return qsTrId("lautta-home-recents")
    case "favourites":
        //% "Favourites"
        return qsTrId("lautta-home-section-favourites")
    case "device":
        //% "On this device"
        return qsTrId("lautta-home-section-device")
    case "android":
        //% "Android"
        return qsTrId("lautta-home-section-android")
    case "volumes":
        //% "Volumes"
        return qsTrId("lautta-home-section-volumes")
    case "servers":
        //% "Servers"
        return qsTrId("lautta-home-section-servers")
    case "nearby":
        //% "Nearby"
        return qsTrId("lautta-home-section-nearby")
    default:
        return ""
    }
}

function sectionSwitchTitle(section) {
    if (section === "recents")
        //% "Newest recents"
        return qsTrId("lautta-home-layout-recents")
    if (section === "volumes")
        //% "Memory cards and USB drives"
        return qsTrId("lautta-home-layout-volumes")
    //% "Servers"
    return qsTrId("lautta-home-layout-servers")
}

function rowName(kind, name) {
    if (kind === "deleted")
        //% "Trash"
        return qsTrId("lautta-home-trash")
    return name
}

// Empty until space is known.
function deviceLine(free, total) {
    return total > 0 ? volumeLine(free, total, "") : ""
}

// Whole number from 10 of its unit up; below, one decimal: there it is a tenth of what is left.
function space(bytes) {
    var value = bytes
    while (value >= 1024)
        value /= 1024
    return Silica.Format.formatFileSize(bytes, value < 9.95 ? 1 : 0)
}

function volumeLine(free, total, fs) {
    var text = ""
    if (free >= 0 && total >= 0) {
        //% "%1 free"
        text = qsTrId("lautta-home-volume-free").arg(space(free))
    } else if (total >= 0) {
        text = space(total)
    }
    if (text.length === 0)
        return fs
    return fs.length > 0 ? text + " · " + fs : text
}

function nearbyLine(provider, host) {
    var parts = []
    if (provider.length > 0)
        parts.push(provider)
    if (host.length > 0)
        parts.push(host)
    return parts.join(" · ")
}

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
