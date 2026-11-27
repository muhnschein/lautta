// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/viewers"
import "../components/viewers/ViewerText.js" as ViewerText

Page {
    id: page

    property string uri
    readonly property string coverKind: "file"
    readonly property string coverUri: uri
    readonly property bool local: App.isLocal(uri)
    readonly property string fileName: App.nameOf(uri)
    readonly property bool playing: playback.playing
    readonly property real position: playback.position
    readonly property real duration: playback.duration
    readonly property string errorKind: playback.errorKind
    readonly property string statusText: playback.statusText
    readonly property bool sideBySide: isLandscape
    readonly property real paneX: sideBySide ? width / 2 : 0
    readonly property real paneWidth: sideBySide ? width / 2 : width
    readonly property string coverUrl: info.cover !== "" ? "image://lautta-cover/" + uri : ""
    readonly property string trackTitle: {
        if (info.title !== "")
            return info.title
        var bare = fileName.replace(/\.[^.]*$/, "")
        return bare !== "" ? bare : fileName
    }

    function toggle() {
        playback.toggle()
    }

    function seek(ms) {
        playback.seek(ms)
    }

    allowedOrientations: Orientation.All

    MediaPlayback {
        id: playback
        uri: page.uri
    }

    ViewerTools { id: tools }

    AudioInfo {
        id: info
        uri: page.uri
    }

    Component.onCompleted: if (local) tools.noteViewed(uri)

    Image {
        id: glow

        objectName: "coverGlow"
        anchors.fill: parent
        source: page.coverUrl !== "" ? page.coverUrl + "?size=48&glow=1" : ""
        fillMode: Image.PreserveAspectCrop
        smooth: true
        asynchronous: true
        opacity: status === Image.Ready ? 1 : 0
        Behavior on opacity { FadeAnimation { } }

        Rectangle {
            anchors.fill: parent
            color: Theme.rgba(Theme.overlayBackgroundColor, 0.55)
        }
    }

    PageHeader {
        id: header
        x: page.paneX
        width: page.paneWidth
        title: page.fileName
        description: page.statusText
    }

    Item {
        id: stage
        x: page.paneX
        width: page.paneWidth
        anchors { top: header.bottom; bottom: controls.top }
    }

    Rectangle {
        id: cover

        objectName: "cover"
        readonly property real block: height + Theme.paddingLarge * 2 + trackInfo.height
        width: page.sideBySide
               ? Math.min(page.width / 2 - 2 * Theme.horizontalPageMargin, page.height - 2 * Theme.paddingLarge)
               : Math.max(0, Math.min(page.width - 2 * Theme.horizontalPageMargin, Theme.itemSizeHuge * 2.4,
                                      stage.height - trackInfo.height - Theme.paddingLarge * 4))
        height: width
        x: page.sideBySide ? (page.width / 2 - width) / 2 : (page.width - width) / 2
        y: page.sideBySide ? (page.height - height) / 2 : stage.y + Math.max(0, (stage.height - block) / 2)
        color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)

        Image {
            anchors.centerIn: parent
            source: "image://theme/icon-l-music?" + Theme.highlightColor
            visible: coverImage.status !== Image.Ready
        }

        Image {
            id: coverImage

            objectName: "coverImage"
            anchors.fill: parent
            source: page.coverUrl !== "" ? page.coverUrl + "?size=" + Math.round(Math.max(cover.width, 16)) : ""
            sourceSize.width: cover.width
            sourceSize.height: cover.height
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            opacity: status === Image.Ready ? 1 : 0
            Behavior on opacity { FadeAnimation { } }
        }
    }

    Column {
        id: trackInfo

        objectName: "trackInfo"
        x: page.paneX + Theme.horizontalPageMargin
        width: page.paneWidth - 2 * Theme.horizontalPageMargin
        y: page.sideBySide ? stage.y + Math.max(0, (stage.height - height) / 2)
                           : cover.y + cover.height + Theme.paddingLarge * 2
        opacity: info.loading ? 0 : 1
        spacing: Theme.paddingSmall
        Behavior on opacity { FadeAnimation { } }

        Label {
            objectName: "trackTitle"
            width: parent.width
            text: page.trackTitle
            textFormat: Text.PlainText
            color: Theme.highlightColor
            font.pixelSize: Theme.fontSizeLarge
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
        }
        Label {
            objectName: "trackArtist"
            width: parent.width
            visible: text !== ""
            text: info.artist
            textFormat: Text.PlainText
            horizontalAlignment: Text.AlignHCenter
            truncationMode: TruncationMode.Fade
        }
        Label {
            objectName: "trackAlbum"
            width: parent.width
            visible: text !== ""
            text: [info.album, info.year > 0 ? String(info.year) : ""].filter(function (s) { return s !== "" }).join(" · ")
            textFormat: Text.PlainText
            color: Theme.secondaryColor
            font.pixelSize: Theme.fontSizeSmall
            horizontalAlignment: Text.AlignHCenter
            truncationMode: TruncationMode.Fade
        }
        Label {
            objectName: "trackNumber"
            width: parent.width
            visible: text !== ""
            text: info.track <= 0 ? ""
                : info.trackTotal >= info.track
                  //% "Track %1 of %2"
                  ? qsTrId("lautta-viewers-media-track-of").arg(info.track).arg(info.trackTotal)
                  //% "Track %1"
                  : qsTrId("lautta-viewers-media-track").arg(info.track)
            textFormat: Text.PlainText
            color: Theme.secondaryColor
            font.pixelSize: Theme.fontSizeSmall
            horizontalAlignment: Text.AlignHCenter
        }
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: playback.loading || (playback.downloading && playback.downloadProgress < 0)
    }

    ProgressBar {
        x: page.paneX
        width: page.paneWidth
        y: page.height - height - Theme.paddingLarge * 2
        visible: playback.downloading && playback.downloadProgress >= 0
        minimumValue: 0
        maximumValue: 1
        value: Math.max(0, playback.downloadProgress)
        //% "Getting the file before playing"
        label: qsTrId("lautta-viewers-media-getting")
    }

    Column {
        id: controls

        x: page.paneX
        width: page.paneWidth
        anchors.bottom: parent.bottom
        visible: !playback.downloading

        Slider {
            id: slider
            width: parent.width
            minimumValue: 0
            maximumValue: Math.max(1, page.duration)
            enabled: page.duration > 0
            handleVisible: page.duration > 0
            onReleased: page.seek(value)

            Binding {
                target: slider
                property: "value"
                value: page.position
                when: !slider.down
            }
        }

        Item {
            width: parent.width
            height: elapsed.height

            Label {
                id: elapsed

                objectName: "elapsedTime"
                x: slider.leftMargin
                text: page.duration > 0 ? ViewerText.clock(slider.value) : ""
                font.pixelSize: Theme.fontSizeSmall
                color: slider.highlighted ? Theme.highlightColor : Theme.primaryColor
            }
            Label {
                objectName: "totalTime"
                x: parent.width - slider.rightMargin - width
                text: page.duration > 0 ? ViewerText.clock(page.duration) : ""
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryColor
            }
        }

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Theme.paddingLarge * 2

            IconButton {
                icon.source: "image://theme/icon-m-previous"
                onClicked: page.seek(0)
            }
            IconButton {
                icon.source: page.playing ? "image://theme/icon-l-pause" : "image://theme/icon-l-play"
                onClicked: page.toggle()
            }
            IconButton {
                icon.source: "image://theme/icon-m-next"
                onClicked: page.seek(page.position + 30000)
            }
        }

        Item { width: 1; height: Theme.paddingLarge }
    }
}
