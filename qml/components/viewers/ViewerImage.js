// SPDX-License-Identifier: LGPL-2.1-or-later
// Image viewer's photo request. ViewerPrefetch asks the same, so Qt's pixmap cache hands its
// decode to the viewer: keep both in step.
.pragma library

function source(uri) {
    return "image://lautta-file/" + uri
}

// Camera's EXIF thumbnail of a local image, or a server image's cached grid thumbnail.
function placeholder(app, uri) {
    return "image://lautta-thumb/" + uri + (app.isLocal(uri) ? "?size=1024&embedded=1" : "?size=1024&cached=1")
}
