// SPDX-License-Identifier: LGPL-2.1-or-later
// Texts of the PDF viewer (PRV-12).
.pragma library

// "12 pages".
function summary(pages) {
    //% "%n page(s)"
    return qsTrId("lautta-viewers-pdf-pages", pages)
}

// Why a file is no PDF the viewer can show (PdfDocument.problem).
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

// True for an http or https address: the only links a PDF may hand to the
// browser.
function isWebLink(url) {
    return /^https?:\/\/\S+$/i.test(url)
}

// The outline item of the section being read: the one going to the latest
// page at or before `page` (from 0), the last of those in reading order; -1
// when there is none.
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

// The page (from 0) typed into "Go to page", or -1 when it is no page
// number of a document with `pages` pages.
function typedPage(text, pages) {
    if (!/^\s*\d+\s*$/.test(text))
        return -1
    var n = parseInt(text, 10)
    return n >= 1 && n <= pages ? n - 1 : -1
}
