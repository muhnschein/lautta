// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import QtMultimedia 5.6 as MM
import Nemo.KeepAlive 1.2
import Lautta 1.0
import "../components/viewers"
import "../components/viewers/ViewerText.js" as ViewerText
import "../components/ErrorText.js" as ErrorText

Page {
    id: page

    property string uri
    property bool video

    readonly property bool local: App.isLocal(uri)
    readonly property string fileName: App.nameOf(uri)
    readonly property bool playing: local ? localPlayer.playbackState === MM.MediaPlayer.PlayingState : remote.playing
    readonly property real position: local ? localPlayer.position : remote.position
    readonly property real duration: local ? localPlayer.duration : remote.duration
    readonly property string errorKind: local ? remote.playerErrorKind(localPlayer.error) : remote.errorKind
    property bool chrome: true
    readonly property bool sideBySide: !video && isLandscape
    readonly property real paneX: sideBySide ? width / 2 : 0
    readonly property real paneWidth: sideBySide ? width / 2 : width
    readonly property string coverUrl: !video && info.cover !== "" ? "image://lautta-cover/" + uri : ""
    readonly property string trackTitle: {
        if (info.title !== "")
            return info.title
        var bare = fileName.replace(/\.[^.]*$/, "")
        return bare !== "" ? bare : fileName
    }

    function toggle() {
        if (local) {
            if (playing)
                localPlayer.pause()
            else
                localPlayer.play()
        } else if (playing) {
            remote.pause()
        } else {
            remote.play()
        }
    }

    function seek(ms) {
        var target = Math.max(0, Math.min(duration > 0 ? duration : ms, ms))
        if (local)
            localPlayer.seek(target)
        else
            remote.seek(target)
    }

    allowedOrientations: Orientation.All

    MM.MediaPlayer {
        id: localPlayer
        source: page.local ? App.localUrl(page.uri) : ""
        autoPlay: true
    }

    MediaSource {
        id: remote
        uri: page.local ? "" : page.uri
        Component.onCompleted: play()
    }

    ViewerTools { id: tools }

    AudioInfo {
        id: info
        uri: page.video ? "" : page.uri
    }

    Component.onCompleted: if (local) tools.noteViewed(uri)
    Component.onDestruction: {
        if (local)
            localPlayer.stop()
        else
            remote.stop()
    }

    DisplayBlanking {
        preventBlanking: page.video && page.playing
    }

    Rectangle {
        anchors.fill: parent
        color: page.video ? "black" : "transparent"
    }

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

    MM.VideoOutput {
        anchors.fill: parent
        visible: page.video
        source: page.local ? localPlayer : remote
        fillMode: MM.VideoOutput.PreserveAspectFit
    }

    MouseArea {
        anchors.fill: parent
        enabled: page.video
        onClicked: page.chrome = !page.chrome
    }

    readonly property string statusText: {
        if (errorKind !== "")
            return ErrorText.message(errorKind, { "item": fileName, "location": App.locationName(uri), "open": true })
        if (remote.downloading)
            //% "Downloading from %1"
            return qsTrId("lautta-viewers-media-downloading").arg(App.locationName(uri))
        if (!local)
            //% "Streaming from %1"
            return qsTrId("lautta-viewers-media-streaming").arg(App.locationName(uri))
        return ""
    }

    PageHeader {
        id: header
        x: page.paneX
        width: page.paneWidth
        opacity: page.chrome || !page.video ? 1 : 0
        visible: opacity > 0
        title: page.fileName
        description: page.statusText
        Behavior on opacity { FadeAnimation { } }
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
        visible: !page.video
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
        visible: !page.video
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
        running: remote.loading || (remote.downloading && remote.downloadProgress < 0)
    }

    ProgressBar {
        x: page.paneX
        width: page.paneWidth
        y: page.video ? (page.height - height) / 2 : page.height - height - Theme.paddingLarge * 2
        visible: remote.downloading && remote.downloadProgress >= 0
        minimumValue: 0
        maximumValue: 1
        value: Math.max(0, remote.downloadProgress)
        //% "Getting the file before playing"
        label: qsTrId("lautta-viewers-media-getting")
    }

    Column {
        id: controls

        x: page.paneX
        width: page.paneWidth
        anchors.bottom: parent.bottom
        opacity: page.chrome || !page.video ? 1 : 0
        visible: opacity > 0 && !remote.downloading
        Behavior on opacity { FadeAnimation { } }

        Slider {
            id: slider
            width: parent.width
            minimumValue: 0
            maximumValue: Math.max(1, page.duration)
            enabled: page.duration > 0
            handleVisible: page.duration > 0
            valueText: ViewerText.clock(value)
            label: page.duration > 0 ? ViewerText.clock(page.duration) : ""
            onReleased: page.seek(value)

            Binding {
                target: slider
                property: "value"
                value: page.position
                when: !slider.down
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
