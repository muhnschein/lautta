// SPDX-License-Identifier: LGPL-2.1-or-later Camera of a scan: each shot becomes a page.
// Portrait only: photo's turn then needs no sensor. `retakeOnly`: one shot for one page, then
// back.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components/scan"
import "../components/scan/ScanMath.js" as ScanMath
import "../components/ErrorText.js" as ErrorText

FullscreenContentPage {
    id: page

    property string parentUri
    // Folder view the scan belongs to; told when the scan is left.
    property Item owner
    property QtObject scan
    // Name typed in the save dialog before more pages were asked for.
    property string name
    property bool retakeOnly
    // Page the next shot replaces, -1: next shot is a new page.
    property int retakeIndex: -1
    // Silica builds this page ahead of showing it: no camera until it is in front.
    property bool shown
    property bool handedOver
    property bool flashOn
    property bool taking
    property bool cameraFailed
    // Page shown still after its shot, null: live picture.
    property var stillPage: null
    property int stillIndex: -1
    // Qt lists no camera before one was opened: known once the viewfinder is built.
    readonly property bool cameraUsable: !cameraFailed && (!finderLoader.item || finderLoader.item.available)
    readonly property bool live: status === PageStatus.Active && Qt.application.state === Qt.ApplicationActive
    readonly property bool working: taking || (scan !== null && scan.busy)
    readonly property int pages: scan ? scan.count : 0

    function shoot() {
        if (stillPage)
            return page.dropStill()
        if (working || !finderLoader.item || !finderLoader.item.ready)
            return
        var path = scan.shotPath()
        if (path.length === 0)
            return
        taking = true
        finderLoader.item.capture(path)
    }

    function photoTaken(path) {
        taking = false
        if (retakeIndex >= 0)
            scan.retake(retakeIndex, path)
        else
            scan.add(path)
        if (!retakeOnly)
            retakeIndex = -1
    }

    function shotFailed() {
        taking = false
        //% "Could not take the picture"
        notice.text = qsTrId("lautta-scan-shot-failed")
        notice.show()
    }

    function showStill(index) {
        stillPage = JSON.parse(scan.pageJson(index))
        stillIndex = stillPage ? index : -1
        stillTimer.restart()
    }

    function dropStill() {
        stillTimer.stop()
        stillPage = null
        stillIndex = -1
    }

    function retakeStill() {
        retakeIndex = stillIndex
        dropStill()
    }

    function adjustStill() {
        var index = stillIndex
        dropStill()
        pageStack.push(Qt.resolvedUrl("../dialogs/ScanAdjustDialog.qml"), { "scan": scan, "index": index })
    }

    function showPages() {
        dropStill()
        pageStack.push(Qt.resolvedUrl("ScanPageView.qml"), { "scan": scan, "index": pages - 1 })
    }

    function finish() {
        dropStill()
        handedOver = true
        pageStack.replace(Qt.resolvedUrl("../dialogs/ScanSaveDialog.qml"),
                          { "parentUri": parentUri, "owner": owner, "scan": scan, "name": name })
    }

    allowedOrientations: Orientation.Portrait
    // As the dialogs around it. Silica's pop between two vertical pages loops its drag binding.
    navigationStyle: PageNavigation.Horizontal
    // Pages taken: no swipe drops them, only the close button.
    backNavigation: retakeOnly || pages === 0

    onStatusChanged: {
        if (status === PageStatus.Active)
            shown = true
        else
            dropStill()
    }
    Component.onDestruction: {
        if (scan && owner && !handedOver)
            owner.endScan(scan, false)
    }

    Connections {
        target: page.scan
        onPhotoReady: {
            if (page.status !== PageStatus.Active)
                return
            if (page.retakeOnly)
                pageStack.pop()
            else
                page.showStill(index)
        }
        onFailed: {
            if (page.status !== PageStatus.Active)
                return
            if (kind === "Unsupported")
                return page.shotFailed()
            notice.text = ErrorText.message(kind, {})
            notice.show()
        }
    }

    // Still stays this long unless tapped away.
    Timer {
        id: stillTimer
        interval: 2500
        onTriggered: page.dropStill()
    }

    Loader {
        id: finderLoader

        objectName: "finderLoader"
        anchors.fill: parent
        active: page.shown && !page.cameraFailed
        sourceComponent: Component {
            ScanViewfinder {
                running: page.live
                flashOn: page.flashOn
                onPhotoSaved: page.photoTaken(path)
                onShotFailed: page.shotFailed()
                onBroken: page.cameraFailed = true
            }
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        visible: placeholder.enabled

        PageHeader {
            //% "Scan"
            title: qsTrId("lautta-scan-title")
            // Clear of the close button.
            rightMargin: Theme.horizontalPageMargin + Theme.itemSizeSmall
        }

        ViewPlaceholder {
            id: placeholder

            objectName: "cameraPlaceholder"
            enabled: page.shown && !page.cameraUsable
            //% "Camera not available"
            text: qsTrId("lautta-scan-no-camera")
            //% "Scanning needs the camera"
            hintText: qsTrId("lautta-scan-no-camera-hint")
        }
    }

    MouseArea {
        id: still

        objectName: "still"
        anchors.fill: parent
        visible: page.stillPage !== null
        onClicked: page.dropStill()

        Rectangle {
            anchors.fill: parent
            color: "black"
        }

        Image {
            id: stillImage

            readonly property var size: ScanMath.fit(still.width, still.height, implicitWidth, implicitHeight, 0)

            anchors.centerIn: parent
            width: size.width
            height: size.height
            source: page.stillPage ? "file://" + page.stillPage.picture : ""
            cache: false

            QuadOutline {
                objectName: "stillOutline"
                anchors.fill: parent
                visible: stillImage.status === Image.Ready
                corners: page.stillPage ? page.stillPage.quad : ScanMath.whole
            }
        }

        Row {
            anchors {
                horizontalCenter: parent.horizontalCenter
                bottom: parent.bottom
                bottomMargin: controls.height + 2 * controls.anchors.bottomMargin
            }
            spacing: Theme.paddingLarge

            Button {
                objectName: "adjustButton"
                //% "Adjust"
                text: qsTrId("lautta-scan-adjust")
                onClicked: page.adjustStill()
            }

            Button {
                objectName: "retakeButton"
                //% "Retake"
                text: qsTrId("lautta-scan-retake")
                onClicked: page.retakeStill()
            }
        }
    }

    IconButton {
        objectName: "flashButton"
        x: Theme.horizontalPageMargin
        y: Theme.paddingLarge
        visible: page.cameraUsable
        icon.source: page.flashOn ? "image://theme/icon-camera-flash-on" : "image://theme/icon-camera-flash-off"
        onClicked: page.flashOn = !page.flashOn
    }

    IconButton {
        objectName: "dismissButton"
        y: Theme.paddingLarge
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        icon.source: "image://theme/icon-m-dismiss"
        onClicked: pageStack.pop()
    }

    Item {
        id: controls

        width: parent.width
        height: shutter.height
        anchors {
            bottom: parent.bottom
            bottomMargin: Theme.paddingLarge * 2
        }

        BackgroundItem {
            id: pagesButton

            objectName: "pagesButton"
            width: Theme.itemSizeLarge
            height: Theme.itemSizeLarge
            anchors {
                left: parent.left
                leftMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            visible: !page.retakeOnly && page.pages > 0
            onClicked: page.showPages()

            Repeater {
                model: page.retakeOnly ? null : page.scan

                Image {
                    readonly property bool last: index === page.pages - 1

                    anchors.fill: parent
                    visible: last
                    fillMode: Image.PreserveAspectFit
                    sourceSize.height: Theme.itemSizeLarge
                    asynchronous: true
                    cache: false
                    source: last ? "file://" + model.view : ""
                }
            }

            Rectangle {
                anchors {
                    right: parent.right
                    top: parent.top
                }
                width: Math.max(height, count.implicitWidth + Theme.paddingMedium)
                height: count.implicitHeight
                radius: height / 2
                color: Theme.highlightBackgroundColor

                Label {
                    id: count

                    objectName: "pageCount"
                    anchors.centerIn: parent
                    font.pixelSize: Theme.fontSizeExtraSmall
                    text: page.pages
                }
            }
        }

        IconButton {
            id: shutter

            objectName: "shutterButton"
            anchors.horizontalCenter: parent.horizontalCenter
            icon.source: "image://theme/icon-camera-shutter"
            visible: page.cameraUsable
            enabled: !page.working
            onClicked: page.shoot()
        }

        IconButton {
            objectName: "doneButton"
            anchors {
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            visible: !page.retakeOnly && page.pages > 0
            enabled: !page.working
            icon.source: "image://theme/icon-m-accept"
            onClicked: page.finish()
        }
    }

    BusyIndicator {
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: page.working
    }

    Notice { id: notice }
}
