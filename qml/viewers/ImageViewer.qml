// SPDX-License-Identifier: LGPL-2.1-or-later
// Image viewer (PRV-4, boards ImageViewer, ImageExif, ImageDeleteRemote):
// swipe through the images of the folder, pinch and double-tap to zoom,
// tap to hide the controls. Local images are shown directly, remote ones
// through image://lautta-file/ (EXIF orientation applied by the provider).
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/viewers"
import "../components/viewers/ViewerText.js" as ViewerText

Page {
    id: page

    property string uri
    // The folder to swipe through; the image's own folder by default.
    property string folderUri: App.parentUri(uri)

    readonly property bool inFolder: images.indexOf(uri) >= 0
    readonly property string currentUri: slideshow.currentItem ? slideshow.currentItem.imageUri : uri
    readonly property int total: inFolder ? images.count : 1
    readonly property int position: inFolder ? Math.max(0, images.indexOf(currentUri)) + 1 : 1
    property bool chrome: true
    // Where the view was when an image was deleted, to land next to it.
    property int keepIndex: -1

    function sourceFor(imageUri) {
        return App.isLocal(imageUri) ? App.localUrl(imageUri) : "image://lautta-file/" + imageUri
    }

    function remove() {
        var target = page.currentUri
        var remote = !App.isLocal(target)
        var seconds = App.setting("remorse_seconds")
        var text = remote
                   //% "Deleting permanently from %1"
                   ? qsTrId("lautta-viewers-delete-remote").arg(App.locationName(target))
                   //% "Deleting"
                   : qsTrId("lautta-viewers-delete")
        remorse.execute(text, function () {
            keepIndex = slideshow.currentIndex
            Operations.remove(JSON.stringify([target]))
            if (page.total <= 1)
                pageStack.pop()
            else
                reloadTimer.restart()
        }, seconds * 1000)
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

        delegate: Item {
            readonly property string imageUri: page.inFolder ? model.uri : page.uri

            width: slideshow.itemWidth
            height: slideshow.height

            ZoomImage {
                anchors.fill: parent
                // Only the shown image and its neighbours load.
                source: Math.abs(index - slideshow.currentIndex) <= 1 ? page.sourceFor(parent.imageUri) : ""
                onTapped: page.chrome = !page.chrome
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
            description: {
                //% "%1 of %2"
                var counter = qsTrId("lautta-viewers-image-count").arg(page.position).arg(page.total)
                return counter + " · " + ViewerText.where(App.locationName(page.currentUri),
                                                               App.displayAddress(App.parentUri(page.currentUri)))
            }
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
