// SPDX-License-Identifier: LGPL-2.1-or-later
// PDF viewer (PRV-12, board PdfViewer): the pages one below the other,
// fitted to the width; pinch or double-tap to zoom, the page number shows
// while scrolling. Opens where reading stopped last time; an encrypted PDF
// asks for its password (board Password dialog). The pulley leads to the
// contents and "Go to page" (board Contents). A link within the PDF jumps
// there, with a way back (board After an internal link); a link to a
// website asks before the browser opens it (board External link); nothing
// in a PDF can reach the network itself. Pages come from
// image://lautta-pdf/, rendered in Rust (hayro); forms are drawn but do
// nothing.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/viewers"
import "../dialogs"
import "../components/ErrorText.js" as ErrorText
import "../components/viewers/PdfText.js" as PdfText

Page {
    id: page

    property string uri
    // Zoom as a multiple of the fitted width; pages are rendered again at
    // `renderZoom` once a pinch has settled.
    property real zoom: 1
    property real renderZoom: 1
    readonly property real maxZoom: 4
    readonly property string fileName: App.nameOf(uri)
    // Why the PDF is not shown (PdfDocument.problem), and whether it loads.
    readonly property string problem: doc.problem
    readonly property bool loading: doc.loading
    readonly property int currentPage: {
        var item = view.itemAt(view.contentX + view.width / 2, view.contentY + view.height / 2)
        return item ? item.pageIndex + 1 : 1
    }

    // Zooms to `to`, keeping the content under (x, y) of the view in place.
    function zoomTo(to, x, y) {
        var z = Math.max(1, Math.min(maxZoom, to))
        var ratio = z / zoom
        var cx = (view.contentX + x) * ratio - x
        var cy = (view.contentY - view.originY + y) * ratio - y + view.originY
        zoom = z
        view.contentX = Math.max(0, Math.min(cx, view.contentWidth - view.width))
        view.contentY = cy
        settle.restart()
    }

    // Scrolls to page `index` (from 0), `top` (a fraction of its height,
    // when not negative) at the top of the view.
    function showPage(index, top) {
        view.positionViewAtIndex(index, ListView.Beginning)
        if (top > 0) {
            var bottom = view.originY + view.contentHeight - view.height
            view.contentY = Math.min(bottom, view.contentY + top * doc.heightFor(index, view.contentWidth))
        }
    }

    // A tapped link: within the PDF it jumps, remembering where reading
    // was; a web link goes to the browser after asking.
    function followLink(link) {
        if (link.page >= 0) {
            backPage = currentPage
            showPage(link.page, link.top)
        } else if (PdfText.isWebLink(link.uri)) {
            pageStack.push(linkDialog, { "url": link.uri })
        }
    }

    function showContents() {
        var contents = pageStack.push(contentsPage, {
            "fileName": fileName,
            "outline": doc.outline !== "" ? JSON.parse(doc.outline) : [],
            "pageCount": doc.pageCount,
            "currentPage": currentPage - 1
        })
        contents.chosen.connect(function(index) {
            pageStack.pop(page)
            showPage(index, -1)
        })
    }

    // Asks for the password of an encrypted PDF; `wrong` after a failed try.
    function askPassword() {
        asked = true
        wantPassword = false
        var dialog = pageStack.push(passwordDialog, { "fileName": fileName, "wrong": doc.wrongPassword })
        dialog.accepted.connect(function() {
            unlocking = true
            doc.unlock(dialog.password)
            dialog.forget()
        })
    }

    function say(kind, message) {
        notice.text = ErrorText.message(kind, { "item": fileName, "location": App.locationName(uri) })
        notice.show()
    }

    allowedOrientations: Orientation.All
    onWidthChanged: { zoom = 1; renderZoom = 1 }

    PdfDocument {
        id: doc
        uri: page.uri

        onStateChanged: {
            if (!loading)
                page.unlocking = false
            if (loaded && lastPage > 0)
                restore.restart()
            else if (problem === "encrypted" && (!page.asked || wrongPassword)) {
                page.wantPassword = true
                askLater.restart()
            }
        }
    }

    // After the pages are laid out: back where reading stopped.
    Timer {
        id: restore
        interval: 0
        onTriggered: view.positionViewAtIndex(doc.lastPage, ListView.Beginning)
    }

    // The password dialog comes when the PDF turns out to be encrypted and
    // again after a wrong password; Cancel leaves it locked (Unlock in the
    // pull-down menu asks again). Pushing waits until this page is shown.
    property bool asked
    property bool wantPassword
    // A password is being tried (board Unlocking).
    property bool unlocking
    // The page (from 1) reading was on before following a link; 0 for none.
    property int backPage
    onStatusChanged: if (status === PageStatus.Active && wantPassword) askLater.restart()
    // While the page stack is still moving, a push would be lost: try again.
    Timer {
        id: askLater
        interval: 50
        onTriggered: {
            if (page.status !== PageStatus.Active || !page.wantPassword)
                return
            if (pageStack.busy)
                restart()
            else
                page.askPassword()
        }
    }

    // Where reading is, saved once scrolling has stopped for a moment.
    onCurrentPageChanged: if (doc.loaded) remember.restart()
    Timer {
        id: remember
        interval: 1000
        onTriggered: doc.rememberPage(page.currentPage - 1)
    }

    Component {
        id: passwordDialog
        PdfPasswordDialog { }
    }

    Component {
        id: linkDialog
        PdfLinkDialog { }
    }

    Component {
        id: contentsPage
        PdfContentsPage { }
    }

    ExternalActions {
        id: external
        uri: page.uri
        onFailed: page.say(kind, message)
    }

    Notice { id: notice }

    Timer {
        id: settle
        interval: 300
        onTriggered: page.renderZoom = page.zoom
    }

    PinchArea {
        anchors.fill: parent
        enabled: doc.loaded

        property real startZoom: 1

        onPinchStarted: startZoom = page.zoom
        onPinchUpdated: page.zoomTo(startZoom * pinch.scale, pinch.center.x, pinch.center.y)
        onPinchFinished: view.returnToBounds()

        SilicaListView {
            id: view

            anchors.fill: parent
            model: doc.loaded ? doc.pageCount : 0
            clip: true
            contentWidth: width * page.zoom
            flickableDirection: page.zoom > 1 ? Flickable.HorizontalAndVerticalFlick : Flickable.VerticalFlick
            spacing: Theme.paddingSmall

            PullDownMenu {
                MenuItem {
                    visible: doc.problem === "encrypted"
                    //% "Unlock"
                    text: qsTrId("lautta-viewers-pdf-unlock")
                    onClicked: page.askPassword()
                }
                MenuItem {
                    //% "Open with"
                    text: qsTrId("lautta-viewers-open-with")
                    onClicked: external.openWith()
                }
                MenuItem {
                    //% "Share"
                    text: qsTrId("lautta-viewers-share")
                    onClicked: external.share()
                }
                MenuItem {
                    visible: doc.loaded
                    //% "Contents and pages"
                    text: qsTrId("lautta-viewers-pdf-contents-and-pages")
                    onClicked: page.showContents()
                }
            }

            header: PageHeader {
                width: view.width
                x: view.contentX
                title: page.fileName
                description: doc.loaded ? PdfText.summary(doc.pageCount) : ""
            }

            delegate: PdfPage {
                id: pdfPage
                pageIndex: index
                uri: page.uri
                width: view.contentWidth
                height: doc.heightFor(index, view.contentWidth)
                fitWidth: view.width
                zoom: page.renderZoom
                onDoubleTapped: {
                    var at = mapToItem(view, tapX, tapY)
                    page.zoomTo(page.zoom > 1 ? 1 : 2.5, at.x, at.y)
                }
                onLinkClicked: page.followLink(link)
                Component.onCompleted: doc.requestLinks(index)

                Connections {
                    target: doc
                    onLinksReady: if (linkPage === pdfPage.pageIndex) pdfPage.links = JSON.parse(pageLinks)
                }
            }

            ViewPlaceholder {
                enabled: doc.problem !== "" || doc.errorKind !== ""
                text: doc.problem !== ""
                      ? PdfText.problem(doc.problem)
                      : ErrorText.message(doc.errorKind, { "item": page.fileName, "location": App.locationName(page.uri) })
                //% "Pull down to open it in another app."
                hintText: doc.problem !== "" ? qsTrId("lautta-viewers-pdf-pull-hint") : ""
            }

            VerticalScrollDecorator { }
            HorizontalScrollDecorator { }
        }
    }

    // Back to where reading was before following a link.
    MouseArea {
        id: back

        anchors { bottom: parent.bottom; bottomMargin: Theme.paddingLarge * 2; horizontalCenter: parent.horizontalCenter }
        width: backRow.width + 2 * Theme.paddingLarge
        height: Theme.itemSizeSmall
        visible: page.backPage > 0 && doc.loaded

        onClicked: {
            page.showPage(page.backPage - 1, -1)
            page.backPage = 0
        }

        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: Theme.rgba(Theme.highlightDimmerColor, 0.9)
            border.color: back.pressed ? Theme.highlightColor : "transparent"
        }

        Row {
            id: backRow
            anchors.centerIn: parent
            spacing: Theme.paddingSmall

            AppIcon {
                anchors.verticalCenter: parent.verticalCenter
                icon: "image://theme/icon-m-back"
                color: Theme.highlightColor
            }
            Label {
                anchors.verticalCenter: parent.verticalCenter
                color: Theme.highlightColor
                //% "Back to page %1"
                text: qsTrId("lautta-viewers-pdf-back-to-page").arg(page.backPage)
            }
        }
    }

    // Page number while the pages move.
    Rectangle {
        anchors {
            bottom: back.visible ? back.top : parent.bottom
            bottomMargin: Theme.paddingLarge * 2
            horizontalCenter: parent.horizontalCenter
        }
        width: counter.implicitWidth + 2 * Theme.paddingLarge
        height: counter.implicitHeight + Theme.paddingSmall * 2
        radius: height / 2
        color: Theme.rgba(Theme.highlightDimmerColor, 0.8)
        opacity: doc.loaded && (view.moving || hideCounter.running) ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { FadeAnimation { } }

        Label {
            id: counter
            anchors.centerIn: parent
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.highlightColor
            //% "Page %1 of %2"
            text: qsTrId("lautta-viewers-pdf-page").arg(page.currentPage).arg(doc.pageCount)
        }

        Timer {
            id: hideCounter
            interval: 1200
        }

        Connections {
            target: view
            onMovingChanged: if (!view.moving) hideCounter.restart()
        }
    }

    Column {
        anchors { bottom: busy.top; bottomMargin: Theme.paddingLarge * 2; horizontalCenter: parent.horizontalCenter }
        spacing: Theme.paddingMedium
        visible: page.unlocking

        HighlightImage {
            anchors.horizontalCenter: parent.horizontalCenter
            source: page.unlocking ? Qt.resolvedUrl("../icons/lock.svg") : ""
            sourceSize.width: Theme.iconSizeLarge
            sourceSize.height: Theme.iconSizeLarge
            color: Theme.highlightColor
        }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            font.pixelSize: Theme.fontSizeExtraLarge
            font.family: Theme.fontFamilyHeading
            color: Theme.highlightColor
            //% "Unlocking"
            text: qsTrId("lautta-viewers-pdf-unlocking")
        }
    }

    BusyIndicator {
        id: busy
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: doc.loading || external.busy
    }
}
