// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library
.import "Json.js" as Json

// As iOS Files: no questions; zip lands beside the items, numbered by core on a clash.
function archiveName(names, isDir) {
    if (names.length !== 1)
        //% "Archive"
        return qsTrId("lautta-compress-default-name")
    return isDir ? names[0] : Json.stem(names[0])
}

function start(app, ops, uris, isDir) {
    var names = uris.map(function (u) { return app.nameOf(u) })
    ops.compress(JSON.stringify(uris), app.parentUri(uris[0]), archiveName(names, isDir), "zip")
}
