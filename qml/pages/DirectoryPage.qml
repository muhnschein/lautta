// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/directory"

Page {
    id: page

    property string uri
    property bool shown
    readonly property bool folderPage: true
    // Set while this page is not on top or the stack still moves.
    property string pendingGoTo
    property bool pendingBrowse

    allowedOrientations: Orientation.All

    onStatusChanged: {
        if (status !== PageStatus.Active)
            return
        goOnIfAsked()
        if (shown)
            view.model.revisit()
        shown = true
    }

    function onTop() {
        return !pageStack.busy && pageStack.currentPage === page
    }

    function goOnIfAsked() {
        if (!onTop())
            return
        if (pendingGoTo.length > 0)
            goTo(pendingGoTo)
        else if (pendingBrowse)
            browse()
    }

    function same(a, b) {
        return App.isInside(a, b) && App.isInside(b, a)
    }

    // Back to the folder's page when on the stack; up the path: the folder replaces the pages
    // inside it; else on top.
    function goTo(target) {
        if (!onTop()) {
            pendingGoTo = target
            return
        }
        pendingGoTo = ""
        var onStack = pageStack.find(function (p) { return p.folderPage === true && same(p.uri, target) })
        if (onStack) {
            if (onStack !== page)
                pageStack.pop(onStack)
            return
        }
        var folder = Qt.resolvedUrl("DirectoryPage.qml")
        if (!App.isInside(uri, target)) {
            pageStack.push(folder, { "uri": target })
            return
        }
        var keep = pageStack.find(function (p) { return !(p.folderPage === true && App.isInside(p.uri, target)) })
        pageStack.replaceAbove(keep, folder, { "uri": target })
    }

    function browse() {
        if (!onTop()) {
            pendingBrowse = true
            return
        }
        pendingBrowse = false
        var home = pageStack.find(function (p) { return p.browsePage === true })
        if (home)
            pageStack.pop(home)
        else
            pageStack.replaceAbove(null, Qt.resolvedUrl("BrowsePage.qml"))
    }

    Connections {
        target: page.pageContainer
        onBusyChanged: page.goOnIfAsked()
    }

    Connections {
        target: Qt.application
        onStateChanged: {
            if (Qt.application.state === Qt.ApplicationActive && page.status === PageStatus.Active)
                view.model.revisit()
        }
    }

    DirectoryView {
        id: view

        anchors.fill: parent
        uri: page.uri
        onOpenFolder: pageStack.push(Qt.resolvedUrl("DirectoryPage.qml"), { "uri": uri })
        onGoTo: page.goTo(uri)
        onBrowse: page.browse()
    }
}
