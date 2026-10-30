// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library
.import "ContextActions.js" as Actions

var endingActions = ["copy", "cut", "share", "compress", "rename", "favourite", "moveToNewFolder"]

function endsSelection(id) {
    return endingActions.indexOf(id) >= 0
}

function start(count) {
    return { "selecting": true, "waiting": count === 0 }
}

function afterCount(state, count) {
    if (count > 0)
        return { "selecting": true, "waiting": false }
    if (state.selecting && state.waiting)
        return state
    return { "selecting": false, "waiting": false }
}

var buttonOrder = ["share", "copy", "cut", "delete"]
var manyOrder = ["compress", "moveToNewFolder"]

function buttonIcon(id) {
    switch (id) {
    case "share": return "image://theme/icon-m-share"
    case "copy": return "dir-copy"
    case "cut": return "dir-cut"
    case "delete": return "image://theme/icon-m-delete"
    case "more": return "more"
    }
    return "image://theme/icon-m-dismiss"
}

function context(writable) {
    return { "isDir": false, "writable": writable }
}

function allowed(order, writable) {
    var ctx = context(writable)
    return order.filter(function (id) { return Actions.applicable(id, ctx) })
}

function moreRows(count, ctx) {
    if (count === 1)
        return Actions.pick(["rename"], ctx).concat(Actions.more(ctx))
    if (count > 1)
        return Actions.more(ctx).filter(function (row) { return manyOrder.indexOf(row.id) >= 0 })
    return []
}

function panelButtons(count, writable) {
    var ids = count > 0 ? allowed(buttonOrder, writable) : []
    if (moreRows(count, context(writable)).length > 0)
        ids.push("more")
    var out = ids.map(function (id) { return { "id": id, "icon": buttonIcon(id), "text": Actions.text(id) } })
    //% "Stop selecting"
    out.push({ "id": "done", "icon": buttonIcon("done"), "text": qsTrId("lautta-dir-sel-done") })
    return out
}

function dragStart(anchor, selected) {
    return { "anchor": anchor, "target": !selected, "from": anchor, "to": anchor - 1 }
}

// `before(i)` = state of item i before drag. Item entering range flips unless already target state;
// one leaving flips back on same condition.
function dragTo(drag, index, before) {
    var next = {
        "anchor": drag.anchor,
        "target": drag.target,
        "from": Math.min(drag.anchor, index),
        "to": Math.max(drag.anchor, index)
    }
    var flips = []
    var take = function (first, last) {
        for (var i = first; i <= last; ++i) {
            if (before(i) !== drag.target)
                flips.push(i)
        }
    }
    take(Math.min(drag.from, next.from), Math.max(drag.from, next.from) - 1)
    take(Math.min(drag.to, next.to) + 1, Math.max(drag.to, next.to))
    return { "drag": next, "flips": flips }
}

function autoScroll(y, top, bottom, zone, fastest) {
    if (y < top + zone)
        return -fastest * Math.min(1, (top + zone - y) / zone)
    if (y > bottom - zone)
        return fastest * Math.min(1, (y - bottom + zone) / zone)
    return 0
}
