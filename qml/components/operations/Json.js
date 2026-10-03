// SPDX-License-Identifier: LGPL-2.1-or-later
// Small helpers shared by the operations pages.
.pragma library

// Objects and JSON text both come in as props (signals pass text).
function value(v, fallback) {
    if (v === undefined || v === null || v === "")
        return fallback
    if (typeof v === "string") {
        try {
            return JSON.parse(v)
        } catch (e) {
            return fallback
        }
    }
    return v
}

// "IMG_2041.jpg" -> "IMG_2041 2.jpg": the name "Keep both" would give.
function keepBothName(name) {
    var m = /^(.*?)((\.tar)?\.[^.\s]{1,16})?$/.exec(name)
    if (!m || m[1].length === 0)
        return name + " 2"
    return m[1] + " 2" + (m[2] || "")
}

function contains(list, v) {
    for (var i = 0; i < list.length; ++i)
        if (list[i] === v)
            return true
    return false
}

// "photos.zip" -> "photos"; used for default archive names.
function stem(name) {
    var m = /^(.+?)(\.tar)?\.[^.\s]{1,16}$/.exec(name)
    return m ? m[1] : name
}
