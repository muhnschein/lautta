// SPDX-License-Identifier: LGPL-2.1-or-later Remote images via image://lautta-file/ (EXIF
// orientation applied by provider).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/viewers"
import "../components/viewers/gallery"
import "../components/ErrorText.js" as ErrorText

FullscreenContentPage {
    id: page

    property string uri
    property string folderUri: App.parentUri(uri)

    property bool listed
    // indexOf is a method: images.count makes the binding follow the listing.
    readonly property bool inFolder: images.count > 0 && (listed || images.indexOf(uri) >= 0)
    readonly property var slideshow: viewLoader.item
    readonly property string currentUri: slideshow && slideshow.currentItem ? slideshow.currentItem.imageUri : uri
    readonly property int total: inFolder ? images.count : 1
    property int keepIndex: -1

    function sourceFor(imageUri) {
        return slideshow.sourceOf(imageUri)
    }

    function remove() {
        var target = page.currentUri
        var deleteIt = function () {
            keepIndex = slideshow.currentIndex
            Operations.remove(JSON.stringify([target]))
        }
        if (Operations.deletesPermanently(JSON.stringify([target]))) {
            // Last image accepted leaves the viewer too: stack cannot pop while dialog closes.
            var props = { "uris": [target] }
            var below = pageStack.previousPage(page)
            if (page.total <= 1 && below) {
                props.acceptDestination = below
                props.acceptDestinationAction = PageStackAction.Pop
            }
            var dialog = pageStack.push(Qt.resolvedUrl("../dialogs/DeleteDialog.qml"), props)
            dialog.accepted.connect(function () {
                deleteIt()
                if (page.total > 1)
                    reloadTimer.restart()
            })
            return
        }
        //% "Moving to trash"
        remorse.execute(qsTrId("lautta-viewers-trashing"), function () {
            deleteIt()
            if (page.total <= 1)
                pageStack.pop()
            else
                reloadTimer.restart()
        }, App.setting("remorse_seconds") * 1000)
    }

    function askRename() {
        var target = page.currentUri
        var dialog = pageStack.replace(Qt.resolvedUrl("../dialogs/RenameDialog.qml"), { "uri": target, "isDir": false })
        dialog.accepted.connect(function () {
            renamer.pendingName = dialog.newName
            renamer.rename(target, dialog.newName)
        })
    }

    function showDetails() {
        pageStack.push(Qt.resolvedUrl("../components/viewers/ImageDetailsPage.qml"),
                       { "uri": page.currentUri, "imageSource": page.sourceFor(page.currentUri) })
    }

    function showMore() {
        var more = pageStack.push(Qt.resolvedUrl("../components/viewers/ImageMorePage.qml"), { "uri": page.currentUri })
        more.renameChosen.connect(page.askRename)
    }

    function say(kind, item) {
        notice.text = ErrorText.message(kind, { "item": item, "location": App.locationName(page.currentUri) })
        notice.show()
    }

    function place() {
        if (page.inFolder && page.keepIndex < 0)
            slideshow.currentIndex = images.indexOf(page.uri)
    }

    allowedOrientations: Orientation.All
    backNavigation: !(slideshow && slideshow.zoomed)

    DirectoryModel {
        id: renamer

        property string pendingName

        onRenamed: {
            page.uri = newUri
            images.reload()
        }
        onRenameFailed: page.say(kind, pendingName)
    }

    ViewerTools { id: tools }

    ExternalActions {
        id: external
        uri: page.currentUri
        onFailed: page.say(kind, App.nameOf(uri))
    }

    Timer {
        id: reloadTimer
        interval: 800
        onTriggered: images.reload()
    }

    Connections {
        target: Operations
        onRemoved: images.reload()
    }

    onCurrentUriChanged: tools.noteViewed(currentUri)

    // New view once listing comes: PagedView slides on model swap; currentIndex skips the slide
    // only as binding at creation.
    Loader {
        id: viewLoader
        anchors.fill: parent
        sourceComponent: page.inFolder ? folderView : imageView
    }

    Component {
        id: imageView
        FlickableImageView {
            objectName: "flickableView"
            model: [page.uri]
            held: remorse.active
        }
    }

    Component {
        id: folderView
        FlickableImageView {
            objectName: "flickableView"
            model: images
            held: remorse.active
            currentIndex: Math.max(0, images.indexOf(page.uri))
        }
    }

    // Declared after the view: children are destroyed in order, PagedView crashes when its model
    // goes first.
    ImageListModel {
        id: images
        folderUri: page.folderUri
        onLoadingChanged: {
            if (loading)
                return
            if (indexOf(page.uri) >= 0)
                page.listed = true
            if (page.keepIndex >= 0) {
                slideshow.currentIndex = Math.min(page.keepIndex, Math.max(0, count - 1))
                page.keepIndex = -1
            } else {
                page.place()
            }
        }
    }

    Connections {
        target: page.slideshow ? page.slideshow.overlay : null
        onDetails: page.showDetails()
        onRemove: page.remove()
        onOpenWith: external.openWith()
        onShare: external.share()
        onMore: page.showMore()
    }

    VerticalPageBackHint {
        pageActive: page.status === PageStatus.Active
    }

    RemorsePopup { id: remorse }

    Notice { id: notice }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: external.busy
    }
}
