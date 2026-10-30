// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

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

function stem(name) {
    var m = /^(.+?)(\.tar)?\.[^.\s]{1,16}$/.exec(name)
    return m ? m[1] : name
}
