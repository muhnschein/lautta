// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library
.import "ContextActions.js" as Actions

var endingActions = ["copy", "cut", "share", "extract", "compress", "rename", "favourite", "moveToNewFolder", "checksums", "verify"]

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

function context(model) {
    return { "isDir": false, "changeable": model.changeable, "readable": model.readable, "writable": model.writable }
}

function allowed(order, ctx) {
    return order.filter(function (id) { return Actions.applicable(id, ctx) })
}

function moreRows(count, ctx) {
    if (count === 1)
        return Actions.pick(["rename"], ctx).concat(Actions.more(ctx))
    if (count > 1)
        return Actions.more(ctx).filter(function (row) { return manyOrder.indexOf(row.id) >= 0 })
    return []
}

function panelButtons(count, ctx) {
    var ids = count > 0 ? allowed(buttonOrder, ctx) : []
    if (moreRows(count, ctx).length > 0)
        ids.push("more")
    var out = ids.map(function (id) { return { "id": id, "icon": buttonIcon(id), "text": Actions.text(id) } })
    //% "Stop selecting"
    out.push({ "id": "done", "icon": buttonIcon("done"), "text": qsTrId("lautta-dir-sel-done") })
    return out
}

function dragStart(anchor, selected) {
    return { "anchor": anchor, "target": !selected, "from": anchor, "to": anchor - 1 }
}

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
