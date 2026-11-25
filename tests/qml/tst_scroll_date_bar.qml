// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components"

ApplicationWindow {
    id: win

    property bool reachedEnd
    readonly property real day: 86400000
    readonly property real now: Date.now()

    ViewerTools { id: tools }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + what)
    }

    function named(name) {
        var found = []
        var walk = function (item) {
            if (!item)
                return
            if (item.objectName === name)
                found.push(item)
            for (var i = 0; i < item.children.length; ++i)
                walk(item.children[i])
        }
        walk(bar)
        return found[0]
    }

    function flickDown() {
        listView.flick(0, -4000)
        return waitFor(function () { return listView.movingVertically && bar.active }, "the list to move")
    }

    function checkShowsEveryTime() {
        expect(!bar.visible && !bar.active, "at rest the pill is hidden")
        if (!flickDown())
            return
        expect(waitFor(function () { return bar.opacity === 1 }, "the pill to show"), "moving shows the pill")
        listView.cancelFlick()
        waitFor(function () { return !listView.movingVertically }, "the list to stop")
        expect(bar.hiding && bar.visible, "stopped, the pill stays a moment before it fades")
        if (!flickDown())
            return
        expect(bar.hiding === false, "moving again while it waits to fade: the fade is called off at once")
        expect(waitFor(function () { return bar.opacity === 1 }, "the pill to show again"), "and the pill is fully back")
        listView.cancelFlick()
        waitFor(function () { return !bar.visible }, "the pill to fade out")
        expect(bar.opacity === 0, "then it fades out")
        if (!flickDown())
            return
        expect(bar.hiding === false, "faded out, moving again shows the pill with no wait")
        waitFor(function () { return bar.opacity === 1 || !listView.movingVertically }, "the pill to show or the list to stop")
        expect(bar.opacity === 1 && listView.movingVertically, "the pill is fully shown while the list still moves")
        listView.cancelFlick()
        waitFor(function () { return !bar.visible }, "the pill to fade out")
    }

    function checkShape() {
        var handle = named("scrollDateHandle")
        var topLeft = named("scrollDateSquareTopLeft")
        var bottomRight = named("scrollDateSquareBottomRight")
        if (!expect(handle && topLeft && bottomRight, "the pill has square top-left and bottom-right corners"))
            return
        expect(topLeft.x === 0 && topLeft.y === 0, "top-left corner square")
        expect(Math.abs(bottomRight.x + bottomRight.width - handle.width) < 0.5
               && Math.abs(bottomRight.y + bottomRight.height - handle.height) < 0.5, "bottom-right corner square")
        var text = named("scrollDateText")
        var description = named("scrollDateDescription")
        expect(text.horizontalAlignment === Text.AlignLeft && description.horizontalAlignment === Text.AlignLeft
               && text.mapToItem(handle, 0, 0).x === description.mapToItem(handle, 0, 0).x,
               "both lines start at the same left edge")
    }

    function checkDragMakesRoom() {
        if (!flickDown())
            return
        waitFor(function () { return bar.opacity === 1 }, "the pill to show")
        listView.cancelFlick()
        var handle = named("scrollDateHandle")
        var text = named("scrollDateText")
        var right = handle.x + handle.width
        var width = handle.width
        var textX = text.mapToItem(bar, 0, 0).x
        var at = handle.mapToItem(bar, handle.width / 2, handle.height / 2)
        tools.mouse(bar, "press", at.x, at.y)
        expect(bar.dragging, "a press on the pill grabs it")
        waitFor(function () { return handle.width >= width + Theme.itemSizeMedium - 0.5 }, "the pill to widen")
        expect(Math.abs(handle.x + handle.width - right) < 0.5, "held, the pill keeps its right edge")
        expect(text.mapToItem(bar, 0, 0).x < textX - Theme.itemSizeMedium + 0.5,
               "and its words move left, clear of the thumb")
        var before = listView.contentY
        for (var i = 1; i <= 6; ++i)
            tools.mouse(bar, "move", at.x, at.y + i * 40)
        expect(listView.contentY > before + 1, "dragging the pill down scrolls the list down")
        tools.mouse(bar, "release", at.x, at.y + 240)
        waitFor(function () { return Math.abs(handle.width - width) < 0.5 }, "the pill to narrow")
    }

    initialPage: Component {
        Page {
            SilicaListView {
                id: view

                anchors.fill: parent
                model: 300
                delegate: ListItem {
                    contentHeight: Theme.itemSizeSmall

                    Label {
                        x: Theme.horizontalPageMargin
                        anchors.verticalCenter: parent.verticalCenter
                        text: index
                    }
                }
                Component.onCompleted: win.listView = view
            }
            ScrollDateBar {
                id: dateBar

                flickable: view
                rowDate: function (index) { return { "inProgress": false, "at": win.now - index * win.day } }
                Component.onCompleted: win.dateBar = dateBar
            }
        }
    }

    property var listView
    property var dateBar
    readonly property var bar: dateBar

    Timer {
        interval: 100
        repeat: true
        running: !win.reachedEnd
        onTriggered: {
            if (!win.listView || !win.dateBar)
                return
            running = false
            win.checkShowsEveryTime()
            win.checkShape()
            win.checkDragMakesRoom()
            win.reachedEnd = true
        }
    }
}
