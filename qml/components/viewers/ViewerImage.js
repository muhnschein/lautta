// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function source(uri) {
    return "image://lautta-file/" + uri
}

function placeholder(app, uri) {
    return "image://lautta-thumb/" + uri + (app.isLocal(uri) ? "?size=1024&embedded=1" : "?size=1024&cached=1")
}

function systemThumbnail(app, uri) {
    if (!app.isLocal(uri))
        return ""
    var path = app.localUrl(uri).substring("file://".length)
    return "image://nemoThumbnail/file://" + path.split("/").map(encodeURIComponent).join("/")
}

function tile(app, uri, size) {
    return size > 0 ? systemThumbnail(app, uri) : ""
}
