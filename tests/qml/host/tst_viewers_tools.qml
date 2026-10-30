// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    property var events: []

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    width: 300
    height: 300

    ViewerTools { id: tools }

    MouseArea {
        id: left

        width: 150
        height: 300
        onPressed: root.events.push("press " + Math.round(mouse.x) + "," + Math.round(mouse.y))
        onPressAndHold: root.events.push("hold")
        onPositionChanged: root.events.push("move " + Math.round(mouse.x) + "," + Math.round(mouse.y))
        onReleased: root.events.push("release")
        onClicked: root.events.push("click")
    }

    MouseArea {
        id: right

        x: 150
        width: 150
        height: 300
        onPressed: root.events.push("right press")
    }

    Component.onCompleted: {
        check(!tools.mouse(left, "wiggle", 1, 1), "unknown kinds are refused")
        check(!tools.mouse(tools, "press", 1, 1), "objects that are not items are refused")
        check(!tools.mouse(null, "release", 1, 1), "no item is refused")
        same(events, [], "nothing sent by refused calls")
        check(tools.mouse(left, "press", 20, 30), "press sent")
        check(left.pressed, "the area under the point is pressed")
        tools.pump(1200)
        check(tools.mouse(right, "move", 10, 40), "move sent")
        check(tools.mouse(left, "release", 160, 40), "release sent")
        same(events, ["press 20,30", "hold", "move 160,40", "release"], "a hold, then a move kept by the grab")
        events = []
        tools.mouse(right, "press", 5, 5)
        tools.mouse(right, "release", 5, 5)
        same(events, ["right press"], "a press goes to the item under the point")
        events = []
        tools.mouse(left, "press", 5, 5)
        tools.mouse(left, "release", 5, 5)
        same(events, ["press 5,5", "release", "click"], "a tap clicks")
    }
}
