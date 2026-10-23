// SPDX-License-Identifier: LGPL-2.1-or-later
// One row of Settings › Browse page (BRW-12), laid out and carried as
// Salama's tab group rows. A tap shows or hides the row on the Browse page
// (its switch tells which). A press on the grip at the right picks the row
// up at once: it follows the finger, the rows it passes move aside, and let
// go it settles into its place. Rows move within their section only, and
// the model moves the row itself as the finger passes a row (moveRow), so
// the list never shows an order that is not stored. Favourites have no
// switch (removing one stays on the Browse page): their star sits where
// the switch is.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "BrowseText.js" as BrowseText

ListItem {
    id: delegate

    // The BrowseLayoutModel of the list.
    property var layoutModel
    // Ys in list coordinates (they do not move with the row); the row the
    // finger is over is counted from startRow, the row picked up.
    property bool carried: false
    property int startRow: -1
    property int firstRow: -1
    property int lastRow: -1
    property real startY: 0
    property real grabY: 0
    property real fingerY: 0

    // The row's roles, set by the list (BrowseLayoutModel).
    property string layoutSection
    property string layoutItem
    property string layoutKind
    property string layoutName
    property bool layoutHidden: false
    property bool layoutMovable: false

    readonly property bool toggles: layoutKind !== "favourite"
    readonly property bool shown: !layoutHidden
    readonly property string title: layoutKind === "nearby"
                                    //% "Servers found nearby"
                                    ? qsTrId("lautta-browse-layout-nearby")
                                    : BrowseText.rowName(layoutKind, layoutName)

    // Asks the page to show or hide the row.
    signal toggled()

    contentHeight: Theme.itemSizeSmall
    // Favourites only move: a tap does nothing, so it lights nothing.
    highlighted: down && toggles
    z: carried ? 1 : 0

    onClicked: if (toggles) toggled()
    // The list laid the carried row out again: keep it under the finger.
    onYChanged: if (carried) follow()
    Component.onDestruction: {
        if (ListView.view && ListView.view.carrier === delegate)
            ListView.view.carrier = null
    }

    // The rows of the section around list row `row`.
    function sectionEnds(row) {
        var first = row
        var last = row
        while (first > 0 && layoutModel.sectionAt(first - 1) === layoutSection)
            --first
        while (last + 1 < layoutModel.count && layoutModel.sectionAt(last + 1) === layoutSection)
            ++last
        firstRow = first
        lastRow = last
    }

    function pickUp(y) {
        startRow = index
        sectionEnds(index)
        startY = delegate.y
        grabY = y
        fingerY = y
        carried = true
        if (ListView.view)
            ListView.view.carrier = delegate
    }

    function follow() {
        body.y = fingerY - grabY - (delegate.y - startY)
    }

    // The row the finger is over is counted from where the carry started,
    // not asked of the list: the list lays its rows out on the next frame,
    // so a second move before then would read stale rows.
    function carryTo(y) {
        if (!carried)
            return
        fingerY = y
        follow()
        var target = startRow + Math.round((y - grabY) / delegate.contentHeight)
        target = Math.max(firstRow, Math.min(lastRow, target))
        if (target !== index)
            layoutModel.moveRow(index, target)
    }

    // The spring is on once the row is let go, before it goes home.
    function drop() {
        carried = false
        body.y = 0
        if (ListView.view && ListView.view.carrier === delegate)
            ListView.view.carrier = null
    }

    Item {
        id: body

        width: parent.width
        height: delegate.contentHeight

        Behavior on y {
            enabled: !delegate.carried

            NumberAnimation {
                duration: 150
                easing.type: Easing.OutQuad
            }
        }

        // The carried row stands out as a pressed one does.
        Rectangle {
            objectName: "layoutCarryWash"
            anchors.fill: parent
            visible: delegate.carried
            color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
        }

        // Where TextSwitch has its switch. The row takes the taps: the
        // switch only shows the state.
        Item {
            id: lead

            x: Theme.horizontalPageMargin - Theme.paddingLarge
            width: Theme.itemSizeExtraSmall
            height: parent.height

            Switch {
                objectName: "layoutSwitch"
                visible: delegate.toggles
                anchors.centerIn: parent
                acceptedButtons: Qt.NoButton
                automaticCheck: false
                checked: delegate.shown
                highlighted: delegate.highlighted || delegate.carried
            }
            HighlightImage {
                visible: !delegate.toggles
                anchors.centerIn: parent
                source: "image://theme/icon-m-favorite"
                highlighted: delegate.carried
            }
        }

        Label {
            anchors {
                left: lead.right
                leftMargin: Theme.colorScheme === Theme.DarkOnLight ? Theme.paddingMedium : 0
                right: grip.visible ? grip.left : parent.right
                rightMargin: grip.visible ? 0 : Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            text: delegate.title
            color: delegate.highlighted || delegate.carried ? Theme.highlightColor : Theme.primaryColor
            truncationMode: TruncationMode.Fade
        }

        // Carries at once, without a hold; preventStealing keeps the list
        // from scrolling under a vertical drag.
        MouseArea {
            id: grip

            objectName: "layoutGrip"
            anchors {
                right: parent.right
                top: parent.top
                bottom: parent.bottom
            }
            width: Theme.itemSizeSmall
            enabled: delegate.layoutMovable
            visible: enabled
            preventStealing: true
            onPressed: delegate.pickUp(mapToItem(delegate.parent, mouse.x, mouse.y).y)
            onPositionChanged: delegate.carryTo(mapToItem(delegate.parent, mouse.x, mouse.y).y)
            onReleased: delegate.drop()
            onCanceled: delegate.drop()

            Column {
                anchors.centerIn: parent
                spacing: Theme.paddingSmall

                Repeater {
                    model: 3

                    Rectangle {
                        width: Theme.iconSizeSmall - Theme.paddingMedium
                        height: Theme._lineWidth
                        radius: height / 2
                        color: grip.pressed || delegate.carried ? Theme.highlightColor : Theme.secondaryColor
                    }
                }
            }
        }
    }
}
