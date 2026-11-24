// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function paths(resources) {
    var out = []
    for (var i = 0; i < resources.length; ++i) {
        var resource = resources[i]
        out.push(typeof resource === "string" ? resource : resource.filePath || resource.url || "")
    }
    return out
}

function probed(shared, files) {
    var out = []
    for (var i = 0; i < files.length; ++i) {
        var file = files[i]
        out.push({ "name": file.name, "uri": file.uri, "readable": file.readable && shared[i].length > 0 })
    }
    return out
}

function pick(entries, readable) {
    var out = []
    for (var i = 0; i < entries.length; ++i) {
        if (entries[i].readable === readable)
            out.push(entries[i])
    }
    return out
}

function readable(entries) {
    return pick(entries, true)
}

function skipped(entries) {
    return pick(entries, false)
}

function uris(entries) {
    var out = []
    for (var i = 0; i < entries.length; ++i)
        out.push(entries[i].uri)
    return out
}

function canSave(files, folder, writable, loading, errorKind) {
    return files > 0 && folder.length > 0 && writable && !loading && errorKind.length === 0
}

function remembered(folder, before) {
    return [folder].concat(before || [])
}

function sectionOf(kind) {
    switch (kind) {
    case "folder": return "device"
    case "volume": return "volumes"
    case "server": return "servers"
    default: return kind
    }
}
