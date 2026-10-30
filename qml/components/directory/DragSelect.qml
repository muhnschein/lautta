// SPDX-License-Identifier: LGPL-2.1-or-later Held delegate stays alive while its row scrolls out of
// view: row is in DelegateModel's persisted items.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "Selection.js" as Selection

Item {
    id: drag

    property Flickable view
    property var model
    property real bottomInset
    property Item holder
    property var range: null
    property var before: ({})
    property point heldAt
    property point finger
    // Scroll waits until finger moves: holding near an edge, or under the opening panel, must not
    // scroll.
    property bool moved
    readonly property real speed: holder && moved
                                  ? Selection.autoScroll(finger.y, 0, view.height - bottomInset,
                                                         Theme.itemSizeMedium, Theme.paddingLarge)
                                  : 0

    function begin(item, index, x, y) {
        var uris = model.selectedUris()
        var was = {}
        for (var i = 0; i < uris.length; ++i)
            was[uris[i]] = true
        before = was
        range = Selection.dragStart(index, was[model.uriAt(index)] === true)
        item.preventStealing = true
        holder = item
        heldAt = view.mapFromItem(item, x, y)
        finger = heldAt
        moved = false
        reach(index)
    }

    function move(x, y) {
        finger = view.mapFromItem(holder, x, y)
        if (Math.abs(finger.x - heldAt.x) + Math.abs(finger.y - heldAt.y) > Theme.startDragDistance)
            moved = true
        follow()
    }

    function end() {
        if (holder)
            holder.preventStealing = false
        range = null
        before = {}
        holder = null
    }

    function follow() {
        var p = view.contentItem.mapFromItem(view, finger.x, finger.y)
        var index = view.itemIndexAt(p.x, p.y)
        if (index >= 0)
            reach(index)
    }

    function reach(index) {
        var step = Selection.dragTo(range, index, function (i) { return drag.before[drag.model.uriAt(i)] === true })
        range = step.drag
        for (var i = 0; i < step.flips.length; ++i)
            model.toggle(model.uriAt(step.flips[i]))
    }

    onHolderChanged: {
        if (!holder && range)
            end()
    }

    Timer {
        interval: 16
        repeat: true
        running: drag.speed !== 0
        onTriggered: {
            var flick = drag.view
            var top = flick.originY
            var bottom = Math.max(top, flick.originY + flick.contentHeight - flick.height)
            var y = Math.max(top, Math.min(bottom, flick.contentY + drag.speed))
            if (y !== flick.contentY) {
                flick.contentY = y
                drag.follow()
            }
        }
    }
}
