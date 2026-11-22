// SPDX-License-Identifier: LGPL-2.1-or-later
// Share on a server file: Getting page first, its file to the folder's share sheet.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win

    readonly property string remote: "lautta://nv-nas/clips/clip.mp4"
    readonly property string fetchedUrl: "file:///home/defaultuser/Downloads/Lautta/Opened/1/clip.mp4"
    property bool scenarioRunning: true
    property bool reachedEnd
    property int step: 0
    property int waited: 0
    property var folderPage
    property var view

    initialPage: Component {
        DirectoryPage {
            uri: "lautta://user-documents/"
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function findAll(item, pred, out) {
        if (!item)
            return out
        if (pred(item))
            out.push(item)
        var kids = item.data || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        return out
    }

    Timer {
        interval: 100
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            if (++win.waited > 100)
                return win.fail("step " + win.step + " timed out")
            switch (win.step) {
            case 0:
                if (pageStack.busy || !pageStack.currentPage)
                    break
                win.folderPage = pageStack.currentPage
                var views = win.findAll(win.folderPage, function (x) {
                    return x.runAction !== undefined && x.share !== undefined
                }, [])
                if (views.length !== 1)
                    return win.fail("one folder view, got " + views.length)
                win.view = views[0]
                win.view.runAction("share", [win.remote], { "mimeType": "" }, null)
                win.step++
                win.waited = 0
                break
            case 1:
                var page = pageStack.currentPage
                if (pageStack.busy || page === win.folderPage)
                    break
                if (page.uri !== win.remote || page.share !== true)
                    return win.fail("Getting page for the shared file, got " + page.uri + " share " + page.share)
                if (pageStack.previousPage(page) !== win.folderPage)
                    return win.fail("Getting page stacks on the folder")
                page.fetched(win.fetchedUrl)
                var shares = win.findAll(win.view, function (x) {
                    return x.trigger !== undefined && x.mimeType !== undefined
                }, [])
                if (shares.length !== 1)
                    return win.fail("one share action, got " + shares.length)
                // ShareAction keeps a file URL as its path.
                var got = shares[0].resources.length === 1 ? String(shares[0].resources[0]) : ""
                if (got !== win.fetchedUrl && "file://" + got !== win.fetchedUrl)
                    return win.fail("share sheet gets the fetched file: " + JSON.stringify(shares[0].resources))
                win.reachedEnd = true
                win.scenarioRunning = false
                break
            }
        }
    }
}
