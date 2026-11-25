// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "ScrollDate.js" as ScrollDate

Item {
    id: bar

    property Flickable flickable
    property var rowDate
    property int shownIndex: -1
    property string text
    property string description
    readonly property bool scrollable: flickable !== null && flickable.contentHeight > flickable.height
    readonly property bool dragging: drag.pressed
    readonly property bool active: scrollable && (flickable.movingVertically || dragging)
    readonly property real range: flickable ? Math.max(1, flickable.contentHeight - flickable.height) : 1
    readonly property real progress: flickable
                                     ? Math.max(0, Math.min(1, (flickable.contentY - flickable.originY) / range))
                                     : 0

    function rowAtTop() {
        var probe = [Theme.paddingSmall, Theme.itemSizeExtraSmall, Theme.itemSizeSmall]
        for (var i = 0; i < probe.length; ++i) {
            var index = flickable.indexAt(flickable.width / 2, flickable.contentY + probe[i])
            if (index >= 0)
                return index
        }
        return -1
    }

    function words(row, nowMs) {
        if (row.inProgress)
            //% "In progress"
            return { "text": qsTrId("lautta-date-in-progress"), "description": "" }
        var date = new Date(row.at)
        var day = Format.formatDate(date, Formatter.DateMediumWithoutYear)
        switch (ScrollDate.kind(row.at, nowMs)) {
        case "today":
            //% "Today"
            return { "text": qsTrId("lautta-date-today"), "description": day }
        case "yesterday":
            //% "Yesterday"
            return { "text": qsTrId("lautta-date-yesterday"), "description": day }
        case "weekday":
            return { "text": Qt.locale().standaloneDayName(date.getDay(), Locale.LongFormat), "description": day }
        default:
            return { "text": Format.formatDate(date, Format.MonthNameStandalone), "description": String(date.getFullYear()) }
        }
    }

    function refresh() {
        if (!active || !rowDate)
            return
        var index = rowAtTop()
        if (index < 0 || index === shownIndex)
            return
        var row = rowDate(index)
        if (!row)
            return
        shownIndex = index
        var shown = words(row, Date.now())
        text = shown.text
        description = shown.description
    }

    objectName: "scrollDateBar"
    anchors.fill: flickable
    opacity: active ? 1 : 0
    visible: opacity > 0
    onActiveChanged: {
        shownIndex = -1
        refresh()
    }

    Behavior on opacity { FadeAnimation { duration: bar.active ? 100 : 400 } }

    Connections {
        target: bar.active ? bar.flickable : null
        onContentYChanged: bar.refresh()
    }

    Rectangle {
        id: handle

        objectName: "scrollDateHandle"
        x: parent.width - width
        y: bar.progress * (parent.height - height)
        width: bar.dragging ? Theme.paddingMedium : Theme.paddingSmall
        height: Theme.itemSizeLarge
        color: Theme.highlightColor
        opacity: bar.dragging ? 1 : Theme.opacityHigh
    }

    Column {
        objectName: "scrollDateLabel"
        anchors {
            right: handle.left
            rightMargin: Theme.paddingLarge
            verticalCenter: handle.verticalCenter
        }

        Label {
            objectName: "scrollDateText"
            anchors.right: parent.right
            text: bar.text
            color: Theme.highlightColor
            font.pixelSize: Theme.fontSizeLarge
        }
        Label {
            objectName: "scrollDateDescription"
            anchors.right: parent.right
            visible: text.length > 0
            text: bar.description
            color: Theme.secondaryHighlightColor
            font.pixelSize: Theme.fontSizeSmall
        }
    }

    MouseArea {
        id: drag

        property real grabbed

        x: parent.width - width
        width: Theme.itemSizeSmall
        height: parent.height
        enabled: bar.visible
        preventStealing: true
        onPressed: grabbed = mouse.y - handle.y
        onPositionChanged: {
            if (!pressed)
                return
            var travel = Math.max(1, bar.height - handle.height)
            var at = Math.max(0, Math.min(1, (mouse.y - grabbed) / travel))
            bar.flickable.cancelFlick()
            bar.flickable.contentY = bar.flickable.originY + at * bar.range
        }
    }
}
