// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function summary(pages) {
    //% "%n pages"
    return qsTrId("lautta-viewers-pdf-pages", pages)
}

function problem(name) {
    switch (name) {
    case "encrypted":
        //% "This PDF is protected with a password."
        return qsTrId("lautta-viewers-pdf-encrypted")
    case "empty":
        //% "This PDF has no pages."
        return qsTrId("lautta-viewers-pdf-empty")
    case "tooLarge":
        //% "This PDF is too large to show here."
        return qsTrId("lautta-viewers-pdf-too-large")
    default:
        //% "This file can't be read as a PDF."
        return qsTrId("lautta-viewers-pdf-invalid")
    }
}

function hit(current, count, page) {
    //% "%1 of %2 · page %3"
    return qsTrId("lautta-viewers-pdf-hit").arg(current).arg(count).arg(page)
}

function notFound(pages) {
    //% "Not found in %n pages"
    return qsTrId("lautta-viewers-pdf-not-found", pages)
}

function isWebLink(url) {
    return /^https?:\/\/\S+$/i.test(url)
}

function currentItem(outline, page) {
    var found = -1
    var best = -1
    for (var i = 0; i < outline.length; ++i) {
        var p = outline[i].page
        if (p >= 0 && p >= best && p <= page) {
            found = i
            best = p
        }
    }
    return found
}

function typedPage(text, pages) {
    if (!/^\s*\d+\s*$/.test(text))
        return -1
    var n = parseInt(text, 10)
    return n >= 1 && n <= pages ? n - 1 : -1
}
