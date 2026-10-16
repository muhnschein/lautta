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
