// SPDX-License-Identifier: LGPL-2.1-or-later
// Selecting by dragging (SPEC §15.3), as in iOS Photos and Immich: hold an
// item, then move the finger over others. The range from the held item to
// the one under the finger, in reading order, takes the held item's new
// state; moving back shrinks it again (Selection.js). Near the top or the
// bottom of the view it scrolls on by itself, faster nearer the edge, and
// the range follows the finger. The held delegate keeps the finger (no
// flick steals it) and reports its moves here; it stays alive while its row
// scrolls out of view because that row is in DelegateModel's persisted
// items while it is held.
import QtQuick 2.6
import Sailfish.Silica 1.0
import "Selection.js" as Selection

Item {
    id: drag

    // The grid or the list shown (DirectoryView.flickable), and its
    // DirectoryModel.
    property Flickable view
    property var model
    // How much of the view's end the docked panel covers.
    property real bottomInset
    // The delegate holding the finger, null when no drag runs.
    property Item holder
    // Selection.dragStart's state, and the uris selected before the drag.
    property var range: null
    property var before: ({})
    // Where the finger held and is, in the view's coordinates.
    property point heldAt
    property point finger
    // Scrolling waits until the finger moves: holding an item near an edge,
    // or one the opening panel slides over, must not scroll the view.
    property bool moved
    readonly property real speed: holder && moved
                                  ? Selection.autoScroll(finger.y, 0, view.height - bottomInset,
                                                         Theme.itemSizeMedium, Theme.paddingLarge)
                                  : 0

    // The delegate `item` of row `index` was held at (x, y) of it.
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

    // The finger moved to (x, y) of the holding delegate.
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

    // The range up to the item under the finger; between items, on the
    // header or past the last item it stays.
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

    // The holder went (the folder was listed anew): the drag is over.
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
