// SPDX-License-Identifier: LGPL-2.1-or-later
// Texts of the transfers area: titles, status lines and reasons, built from
// the raw numbers and engineering names the Transfers models give (QML
// formats sizes and times, doc/QML-API.md).
.pragma library
.import Sailfish.Silica 1.0 as Silica
.import "../PlaceText.js" as PlaceText

function size(bytes) {
    return Silica.Format.formatFileSize(bytes)
}

function duration(seconds) {
    return Silica.Format.formatDuration(seconds, Silica.Formatter.DurationShort)
}

function when(ms) {
    return Silica.Format.formatDate(new Date(ms), Silica.Formatter.TimepointRelative)
}

function percent(done, total) {
    return total > 0 ? Math.min(100, Math.floor(done * 100 / total)) : 0
}

// The three forms of a task's name: what to do, what runs, what ended.
var IMPERATIVE = 0
var ONGOING = 1
var ENDED = 2

function withArgs(texts, first, second) {
    return texts.map(function (text) {
        var t = first === undefined ? text : text.arg(first)
        return second === undefined ? t : t.arg(second)
    })
}

// r: { kind, direction, title, itemsTotal, destName }
function forms(r) {
    var one = r.itemsTotal <= 1
    var n = r.itemsTotal
    switch (r.kind) {
    case "copy":
        if (one && r.direction === "download")
            return withArgs([
                //% "Download %1"
                qsTrId("lautta-xfr-title-download-one"),
                //% "Downloading %1"
                qsTrId("lautta-xfr-title-downloading-one"),
                //% "Downloaded %1"
                qsTrId("lautta-xfr-title-downloaded-one")], r.title)
        if (one && r.direction !== "upload")
            return withArgs([
                //% "Copy %1 to %2"
                qsTrId("lautta-xfr-title-copy-one"),
                //% "Copying %1 to %2"
                qsTrId("lautta-xfr-title-copying-one"),
                //% "Copied %1 to %2"
                qsTrId("lautta-xfr-title-copied-one")], r.title, r.destName)
        if (!one)
            return withArgs([
                //% "Copy %n item(s) to %1"
                qsTrId("lautta-xfr-title-copy-many", n),
                //% "Copying %n item(s) to %1"
                qsTrId("lautta-xfr-title-copying-many", n),
                //% "Copied %n item(s) to %1"
                qsTrId("lautta-xfr-title-copied-many", n)], r.destName)
        break
    case "move":
        if (one)
            return withArgs([
                //% "Move %1 to %2"
                qsTrId("lautta-xfr-title-move-one"),
                //% "Moving %1 to %2"
                qsTrId("lautta-xfr-title-moving-one"),
                //% "Moved %1 to %2"
                qsTrId("lautta-xfr-title-moved-one")], r.title, r.destName)
        return withArgs([
            //% "Move %n item(s) to %1"
            qsTrId("lautta-xfr-title-move-many", n),
            //% "Moving %n item(s) to %1"
            qsTrId("lautta-xfr-title-moving-many", n),
            //% "Moved %n item(s) to %1"
            qsTrId("lautta-xfr-title-moved-many", n)], r.destName)
    case "delete":
        if (one)
            return withArgs([
                //% "Delete %1"
                qsTrId("lautta-xfr-title-delete-one"),
                //% "Deleting %1"
                qsTrId("lautta-xfr-title-deleting-one"),
                //% "Deleted %1"
                qsTrId("lautta-xfr-title-deleted-one")], r.title)
        return [
            //% "Delete %n item(s)"
            qsTrId("lautta-xfr-title-delete-many", n),
            //% "Deleting %n item(s)"
            qsTrId("lautta-xfr-title-deleting-many", n),
            //% "Deleted %n item(s)"
            qsTrId("lautta-xfr-title-deleted-many", n)]
    case "compress":
        if (one)
            return withArgs([
                //% "Compress %1"
                qsTrId("lautta-xfr-title-compress-one"),
                //% "Compressing %1"
                qsTrId("lautta-xfr-title-compressing-one"),
                //% "Compressed %1"
                qsTrId("lautta-xfr-title-compressed-one")], r.title)
        return [
            //% "Compress %n item(s)"
            qsTrId("lautta-xfr-title-compress-many", n),
            //% "Compressing %n item(s)"
            qsTrId("lautta-xfr-title-compressing-many", n),
            //% "Compressed %n item(s)"
            qsTrId("lautta-xfr-title-compressed-many", n)]
    case "extract":
        return withArgs([
            //% "Extract %1 to %2"
            qsTrId("lautta-xfr-title-extract"),
            //% "Extracting %1 to %2"
            qsTrId("lautta-xfr-title-extracting"),
            //% "Extracted %1 to %2"
            qsTrId("lautta-xfr-title-extracted")], r.title, r.destName)
    default:
        break
    }
    return withArgs([
        //% "Upload %1 to %2"
        qsTrId("lautta-xfr-title-upload-one"),
        //% "Uploading %1 to %2"
        qsTrId("lautta-xfr-title-uploading-one"),
        //% "Uploaded %1 to %2"
        qsTrId("lautta-xfr-title-uploaded-one")], r.title, r.destName)
}

// "Copy 37 items to NAS", "Upload figure-3.png to NAS", "Download holiday.mp4";
// completed ones in the past tense ("Deleted 4 items").
// r: { kind, direction, state, title, itemsTotal, destName }
function title(r) {
    return forms(r)[r.state === "completed" ? ENDED : IMPERATIVE]
}

// The title split around the destination's name (PlaceText.around), for
// a line that marks a server's name with its icon (§15.5).
function titleParts(r) {
    return PlaceText.around(function (dest) {
        var marked = {}
        for (var key in r)
            marked[key] = r[key]
        marked.destName = dest
        return title(marked)
    }, r.destName)
}

// Whether a transfer's destination is on a server (LOC-7): uploads and
// transfers between servers go to one.
function toServer(direction) {
    return direction === "upload" || direction === "remote"
}

// "Copied 37 items to NAS", "Deleted notes.md", "Extracted dataset.tar.gz".
function ended(r) {
    return forms(r)[ENDED]
}

// "Copying 37 items to NAS", "Deleting notes.md": what runs now.
function ongoing(r) {
    return forms(r)[ONGOING]
}

// "1.2 GB of 4.0 GB · 11.4 MB/s · 4 min"
function progressLine(done, total, rate, eta) {
    //% "%1 of %2"
    var text = qsTrId("lautta-xfr-progress").arg(size(done)).arg(size(total))
    if (rate > 0)
        //% "%1/s"
        text += " · " + qsTrId("lautta-xfr-rate").arg(size(rate))
    if (rate > 0 && eta >= 0)
        text += " · " + duration(eta)
    return text
}

// "about 4 min left" for the details page.
function etaLine(rate, eta) {
    if (rate <= 0)
        return ""
    var text = qsTrId("lautta-xfr-rate").arg(size(rate))
    if (eta >= 0)
        //% "about %1 left"
        text += " · " + qsTrId("lautta-xfr-eta-left").arg(duration(eta))
    return text
}

function waitReason(reason) {
    switch (reason) {
    case "network":
        //% "Waiting for network"
        return qsTrId("lautta-xfr-wait-network")
    case "volume":
        //% "Insert the card to continue"
        return qsTrId("lautta-xfr-wait-volume")
    case "bridge":
        //% "Waiting for the connection"
        return qsTrId("lautta-xfr-wait-bridge")
    default:
        //% "Waiting for your answer"
        return qsTrId("lautta-xfr-wait-question")
    }
}

function needsAnswer(questions) {
    //% "Needs an answer: %n conflict(s)"
    return qsTrId("lautta-xfr-needs-answer", questions)
}

// The second line of a transfer row. r is a TransfersModel row.
// restored: add "resumes from where it stopped" to paused rows.
function status(r, restored) {
    switch (r.group) {
    case "active":
        if (r.state === "scanning")
            //% "Scanning… %n item(s)"
            return qsTrId("lautta-xfr-scanning", r.itemsTotal)
        if (r.state === "queued" && r.bytesDone === 0)
            //% "Queued"
            return qsTrId("lautta-xfr-queued")
        return progressLine(r.bytesDone, r.bytesTotal, r.rate, r.eta)
    case "waiting":
        return waitReason(r.waitReason)
    case "paused":
        //% "Paused at %1%"
        var paused = qsTrId("lautta-xfr-paused-at").arg(percent(r.bytesDone, r.bytesTotal))
        //% "resumes from where it stopped"
        return restored ? paused + " · " + qsTrId("lautta-xfr-resumes") : paused
    default:
        return historyLine(r)
    }
}

function historyLine(r) {
    var at = r.finishedMs >= 0 ? when(r.finishedMs) : ""
    var text
    if (r.state === "canceled")
        //% "Canceled"
        text = qsTrId("lautta-xfr-canceled")
    else if (r.state === "failed" && r.itemsFailed > 0 && r.itemsDone > 0)
        //% "Completed with %n failure(s)"
        text = qsTrId("lautta-xfr-completed-failures", r.itemsFailed)
    else if (r.state === "failed")
        //% "Failed"
        text = qsTrId("lautta-xfr-failed")
    else
        //% "Completed"
        text = qsTrId("lautta-xfr-completed")
    return at.length > 0 ? text + " · " + at : text
}

function directionIcon(direction) {
    switch (direction) {
    case "upload":
        return "image://theme/icon-m-cloud-upload"
    case "download":
        return "image://theme/icon-m-cloud-download"
    case "delete":
        return "image://theme/icon-m-delete"
    default:
        return "image://theme/icon-m-transfer"
    }
}

function itemIcon(state) {
    switch (state) {
    case "done":
        return "image://theme/icon-m-acknowledge"
    case "failed":
        return "image://theme/icon-m-warning"
    case "skipped":
        return "image://theme/icon-m-dismiss"
    case "needs-answer":
        return "image://theme/icon-m-question"
    case "running":
        return "image://theme/icon-m-transfer"
    default:
        return "image://theme/icon-m-clock"
    }
}

// "Uploading · 6.8 MB", "Queued · 5.1 MB", "Completed · 7.2 MB" (details page).
function itemStatus(state, kind, sizeBytes, direction) {
    var label
    switch (state) {
    case "done":
        //% "Completed"
        label = qsTrId("lautta-xfr-completed")
        break
    case "running":
        label = direction === "upload"
            //% "Uploading"
            ? qsTrId("lautta-xfr-item-uploading")
            : direction === "download"
                //% "Downloading"
                ? qsTrId("lautta-xfr-item-downloading")
                //% "Copying"
                : qsTrId("lautta-xfr-item-copying")
        break
    case "skipped":
        //% "Skipped"
        label = qsTrId("lautta-xfr-item-skipped")
        break
    case "needs-answer":
        //% "Needs an answer"
        label = qsTrId("lautta-xfr-item-needs-answer")
        break
    default:
        //% "Queued"
        label = qsTrId("lautta-xfr-queued")
    }
    return kind === "dir" || sizeBytes <= 0 ? label : label + " · " + size(sizeBytes)
}

// The activity bar (XFR-13). mode is Transfers.barMode, s the lead transfer's
// summary (Transfers.summaryJson), others how many more are unfinished.
function barTitle(mode, s) {
    if (mode === "done")
        return ended(s)
    if (mode === "failed")
        return title(s)
    var text = ongoing(s)
    return mode === "running" && s.bytesTotal > 0 ? text + " · " + percent(s.bytesDone, s.bytesTotal) + "%" : text
}

// "120 MB of 290 MB · 1 min · 2 more", "Waiting for your answer", "Completed with 1 failure".
function barDetail(mode, s, others, questions) {
    var text
    switch (mode) {
    case "question":
        text = needsAnswer(Math.max(1, questions))
        break
    case "waiting":
        text = waitReason(s.waitReason)
        break
    case "paused":
        text = qsTrId("lautta-xfr-paused-at").arg(percent(s.bytesDone, s.bytesTotal))
        break
    case "running":
        if (s.state === "scanning")
            text = qsTrId("lautta-xfr-scanning", s.itemsTotal)
        else if (s.bytesTotal > 0)
            text = progressLine(s.bytesDone, s.bytesTotal, s.rate, s.eta)
        else
            //% "%1 of %2 items"
            text = qsTrId("lautta-xfr-items-progress").arg(s.itemsDone).arg(s.itemsTotal)
        break
    default:
        return historyLine({ "state": s.state, "itemsFailed": s.itemsFailed, "itemsDone": s.itemsDone, "finishedMs": -1 })
    }
    //% "%n more"
    return others > 0 ? text + " · " + qsTrId("lautta-xfr-bar-more", others) : text
}

function barIcon(mode, direction) {
    switch (mode) {
    case "done":
        return "image://theme/icon-m-acknowledge"
    case "failed":
    case "question":
        return "image://theme/icon-m-warning"
    case "paused":
        return "image://theme/icon-m-pause"
    default:
        return directionIcon(direction)
    }
}
