// SPDX-License-Identifier: LGPL-2.1-or-later
// Image viewer's photo request. ViewerPrefetch and Image details' preview ask the same, so Qt's
// pixmap cache shares one decode: keep all three in step.
.pragma library

function source(uri) {
    return "image://lautta-file/" + uri
}

// Local image: camera's EXIF thumbnail, else clear image of the photo's shape (tile goes under
// it). Server image: its cached grid thumbnail.
function placeholder(app, uri) {
    return "image://lautta-thumb/" + uri + (app.isLocal(uri) ? "?size=1024&embedded=1" : "?size=1024&cached=1")
}

// Local image's grid tile as the system thumbnailer cached it for a `size` request: square crop.
// Its file URL percent-encoded, else names with spaces or '#' fail.
function tile(app, uri, size) {
    if (size <= 0 || !app.isLocal(uri))
        return ""
    var path = app.localUrl(uri).substring("file://".length)
    return "image://nemoThumbnail/file://" + path.split("/").map(encodeURIComponent).join("/")
}
