// SPDX-License-Identifier: LGPL-2.1-or-later
// Image viewer (PRV-4, boards ImageViewer, ImageExif, ImageDeleteRemote):
// Jolla Gallery's full-screen viewer (components/viewers/gallery, BSD-3-Clause,
// after jolla-gallery's GalleryFullscreenPage). The folder's images in its
// order, swiped sideways; a tap shows or hides the overlay (details, close,
// Delete, Open with, Share, More), a pinch or a double tap zooms and puts the
// overlay away, a tap zooms out again; a swipe down or up goes back. Local
// images are shown directly, remote ones through image://lautta-file/ (EXIF
// orientation applied by the provider).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/viewers"
import "../components/viewers/gallery"
import "../components/ErrorText.js" as ErrorText

FullscreenContentPage {
    id: page

    property string uri
    // The folder to swipe through; the image's own folder by default.
    property string folderUri: App.parentUri(uri)

    // The listing has had the image: the view stays on the folder when the
    // image is deleted (to its neighbour) or renamed.
    property bool listed
    // indexOf is a method: images.count makes the binding follow the listing.
    readonly property bool inFolder: images.count > 0 && (listed || images.indexOf(uri) >= 0)
    readonly property string currentUri: slideshow.currentItem ? slideshow.currentItem.imageUri : uri
    readonly property int total: inFolder ? images.count : 1
    // Where the view was when an image was deleted, to land next to it.
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
            // For good (volumes, servers, the Trash off, OPS-8): ask, with no
            // timer after. Accepting for the last image leaves the viewer
            // too: the stack cannot pop while the dialog closes.
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

    // More › Rename: the dialog only asks, so the viewer renames, and keeps
    // showing the image under its new name.
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

    // On the image the viewer opened with (or renamed) once the folder's
    // listing shows it. Called when the view takes up the listing as well:
    // that may come after the listing's own change.
    function place() {
        if (page.inFolder && page.keepIndex < 0)
            slideshow.currentIndex = images.indexOf(page.uri)
    }

    allowedOrientations: Orientation.All
    // Zoomed in, drags pan the image; a tap zooms out and swipes go back.
    backNavigation: !slideshow.zoomed

    // Renames only (no folder listed).
    DirectoryModel {
        id: renamer

        property string pendingName

        onRenamed: {
            App.noteUndo()
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

    // The images, swiped, with the overlay over them.
    FlickableImageView {
        id: slideshow

        objectName: "flickableView"
        anchors.fill: parent
        model: page.inFolder ? images : [page.uri]
        held: remorse.active
        onModelChanged: page.place()
    }

    // Declared after the view: the page's children go in this order when it
    // is destroyed, and Silica's PagedView crashes when its model went first.
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
        target: slideshow.overlay
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
