// SPDX-License-Identifier: LGPL-2.1-or-later
// Image viewer (PRV-4, boards ImageViewer, ImageExif, ImageDeleteRemote):
// swipe through the images of the folder, pinch and double-tap to zoom,
// tap to hide the controls. Local images are shown directly, remote ones
// through image://lautta-file/ (EXIF orientation applied by the provider).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/viewers"

Page {
    id: page

    property string uri
    // The folder to swipe through; the image's own folder by default.
    property string folderUri: App.parentUri(uri)

    // indexOf is a method: images.count makes the binding follow the listing.
    readonly property bool inFolder: images.count > 0 && images.indexOf(uri) >= 0
    readonly property string currentUri: slideshow.currentItem ? slideshow.currentItem.imageUri : uri
    readonly property int total: inFolder ? images.count : 1
    readonly property int position: inFolder && images.count > 0 ? Math.max(0, images.indexOf(currentUri)) + 1 : 1
    property bool chrome: true
    // Where the view was when an image was deleted, to land next to it.
    property int keepIndex: -1

    function sourceFor(imageUri) {
        return App.isLocal(imageUri) ? App.localUrl(imageUri) : "image://lautta-file/" + imageUri
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

    allowedOrientations: Orientation.All
    backNavigation: true

    ImageListModel {
        id: images
        folderUri: page.folderUri
        onLoadingChanged: {
            if (loading)
                return
            if (page.keepIndex >= 0) {
                slideshow.currentIndex = Math.min(page.keepIndex, Math.max(0, count - 1))
                page.keepIndex = -1
            } else if (page.inFolder) {
                slideshow.currentIndex = indexOf(page.uri)
            }
        }
    }

    ListModel {
        id: single
        ListElement { uri: "" }
    }

    ViewerTools { id: tools }

    ExternalActions {
        id: external
        uri: page.currentUri
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

    Rectangle {
        anchors.fill: parent
        color: "black"
    }

    SlideshowView {
        id: slideshow

        anchors.fill: parent
        itemWidth: width
        model: page.inFolder ? images : single
        // Zoomed in or pinching, drags pan the image instead of swiping.
        interactive: !(currentItem && currentItem.holdsGestures)

        delegate: Item {
            readonly property string imageUri: page.inFolder ? model.uri : page.uri
            readonly property bool holdsGestures: zoomImage.holdsGestures

            width: slideshow.itemWidth
            height: slideshow.height

            ZoomImage {
                id: zoomImage
                anchors.fill: parent
                // Only the shown image and its neighbours load.
                source: Math.abs(index - slideshow.currentIndex) <= 1 ? page.sourceFor(parent.imageUri) : ""
                onTapped: page.chrome = !page.chrome
            }

            // Swiped away: back to the fitted size for when it returns.
            Connections {
                target: slideshow
                onCurrentIndexChanged: if (index !== slideshow.currentIndex) zoomImage.resetZoom()
            }
        }
    }

    // Header over a dark gradient.
    Rectangle {
        anchors { top: parent.top; left: parent.left; right: parent.right }
        height: Theme.itemSizeHuge
        opacity: page.chrome ? 1 : 0
        visible: opacity > 0
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0.7) }
            GradientStop { position: 1; color: "transparent" }
        }
        Behavior on opacity { FadeAnimation { } }

        PageHeader {
            title: App.nameOf(page.currentUri)
            //% "%1 of %2"
            description: qsTrId("lautta-viewers-image-count").arg(page.position).arg(page.total)
        }
    }

    // Actions over a dark gradient.
    Rectangle {
        anchors { bottom: parent.bottom; left: parent.left; right: parent.right }
        height: Theme.itemSizeHuge
        opacity: page.chrome ? 1 : 0
        visible: opacity > 0
        gradient: Gradient {
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0.75) }
        }
        Behavior on opacity { FadeAnimation { } }

        Row {
            anchors { bottom: parent.bottom; horizontalCenter: parent.horizontalCenter }
            height: Theme.itemSizeLarge
            width: parent.width

            Repeater {
                model: [
                    { "icon": "image://theme/icon-m-share", "id": "share" },
                    { "icon": "image://theme/icon-m-about", "id": "details" },
                    { "icon": "image://theme/icon-m-edit", "id": "open" },
                    { "icon": "image://theme/icon-m-delete", "id": "delete" },
                    { "icon": "image://theme/icon-m-other", "id": "more" }
                ]

                IconButton {
                    width: parent.width / 5
                    anchors.verticalCenter: parent.verticalCenter
                    icon.source: modelData.icon
                    icon.color: "white"
                    icon.highlightColor: Theme.highlightColor
                    onClicked: {
                        switch (modelData.id) {
                        case "share":
                            external.share()
                            break
                        case "details":
                            pageStack.push(Qt.resolvedUrl("../components/viewers/ImageDetailsPage.qml"),
                                           { "uri": page.currentUri, "imageSource": page.sourceFor(page.currentUri) })
                            break
                        case "open":
                            external.openWith()
                            break
                        case "delete":
                            page.remove()
                            break
                        default:
                            pageStack.push(Qt.resolvedUrl("../components/viewers/ImageMorePage.qml"),
                                           { "uri": page.currentUri })
                        }
                    }
                }
            }
        }
    }

    RemorsePopup { id: remorse }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: external.busy
    }
}
