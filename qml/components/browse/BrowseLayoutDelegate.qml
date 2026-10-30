// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "BrowseText.js" as BrowseText

ListItem {
    id: delegate

    property var layoutModel
    property bool carried: false
    property int startRow: -1
    property int firstRow: -1
    property int lastRow: -1
    property real startY: 0
    property real grabY: 0
    property real fingerY: 0

    property string layoutSection
    property string layoutItem
    property string layoutKind
    property string layoutName
    property bool layoutHidden: false
    property bool layoutMovable: false
    property bool layoutNested: false

    readonly property bool toggles: layoutKind !== "favourite"
    readonly property bool shown: !layoutHidden
    readonly property real indent: layoutNested ? 2 * Theme.paddingLarge : 0
    readonly property string title: {
        switch (layoutKind) {
        case "nearby":
            //% "Servers found nearby"
            return qsTrId("lautta-browse-layout-nearby")
        case "section":
            return BrowseText.sectionSwitchTitle(layoutSection)
        default:
            return BrowseText.rowName(layoutKind, layoutName)
        }
    }

    signal toggled()

    contentHeight: Theme.itemSizeSmall
    highlighted: down && toggles
    z: carried ? 1 : 0

    onClicked: if (toggles) toggled()
    onYChanged: if (carried) follow()
    Component.onDestruction: {
        if (ListView.view && ListView.view.carrier === delegate)
            ListView.view.carrier = null
    }

    function sectionEnds(row) {
        var first = row
        var last = row
        while (first > 0 && layoutModel.sectionAt(first - 1) === layoutSection && layoutModel.placeAt(first - 1) >= 0)
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

    // Row under finger counted from carry start, not asked of list: list lays out next frame, a
    // second move would read stale rows.
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

        Rectangle {
            objectName: "layoutCarryWash"
            anchors.fill: parent
            visible: delegate.carried
            color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
        }

        Item {
            id: lead

            x: Theme.horizontalPageMargin - Theme.paddingLarge + delegate.indent
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

        // preventStealing: list does not scroll under a vertical drag.
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
