// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function field(row, name, fallback) {
    return row[name] === undefined || row[name] === null ? fallback : row[name]
}

function told(row, app) {
    if (row.thumbnailSource !== undefined || !row.uri)
        return row
    var json = app.tileOf(row.uri)
    return json.length > 0 ? JSON.parse(json) : row
}
