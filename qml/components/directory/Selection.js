// SPDX-License-Identifier: LGPL-2.1-or-later
// Selection mode (SPEC §15.3, board DirectorySelect): when it starts and
// ends, what its panel and its More actions page offer and how a drag selects
// a range.
// Picking the first item (holding one in grid view picks it) starts it;
// dropping the last one ends it, unless the mode was started with nothing
// picked yet (pulley Select), which waits for the first tap. The panel's
// dismiss button and the actions end it too.
.pragma library
.import "ContextActions.js" as Actions

// The actions after which the selection is done, Rename and Add to
// favourites included: their dialog takes over the one item. Delete ends it
// once its remorse timer has run out, so a cancelled one keeps the selection.
// More actions only opens a page: leaving it keeps the selection.
var endingActions = ["copy", "cut", "share", "compress", "rename", "favourite"]

function endsSelection(id) {
    return endingActions.indexOf(id) >= 0
}

// The mode turned on by hand with `count` items already selected.
function start(count) {
    return { "selecting": true, "waiting": count === 0 }
}

// The mode after the number of selected items changed to `count`; `state`
// is { selecting, waiting }.
function afterCount(state, count) {
    if (count > 0)
        return { "selecting": true, "waiting": false }
    if (state.selecting && state.waiting)
        return state
    return { "selecting": false, "waiting": false }
}

// ---- The panel ----

// The panel's buttons in the long-press menu's order (ContextActions.js),
// then More actions and the dismiss button. Rename and Add to favourites are
// for one item: with one selected they are rows of the More actions page
// (moreRows), which grid view needs, as holding an item selects it there.
var buttonOrder = ["share", "copy", "cut", "delete"]
// The rows of the More actions page that work on several items.
var manyOrder = ["compress"]

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

// The context of the actions of items in a folder that is `writable` or
// not; the first selected item's kind changes none of them.
function context(writable) {
    return { "isDir": false, "writable": writable }
}

// The ids of `order` that a folder, `writable` or not, allows.
function allowed(order, writable) {
    var ctx = context(writable)
    return order.filter(function (id) { return Actions.applicable(id, ctx) })
}

// [{ id, text, icon }] of the More actions page of `count` selected items,
// the first described by `ctx` ({ isDir, writable }): with one item Rename
// and every row of that item's own More actions page (ContextActions.js);
// with several, the rows among them that work on several items.
function moreRows(count, ctx) {
    if (count === 1)
        return Actions.pick(["rename"], ctx).concat(Actions.more(ctx))
    if (count > 1)
        return Actions.more(ctx).filter(function (row) { return manyOrder.indexOf(row.id) >= 0 })
    return []
}

// [{ id, icon, text }] for `count` selected items of a folder that is
// `writable` or not; "more" opens the More actions page, "done" leaves
// selection.
function panelButtons(count, writable) {
    var ids = count > 0 ? allowed(buttonOrder, writable) : []
    if (moreRows(count, context(writable)).length > 0)
        ids.push("more")
    var out = ids.map(function (id) { return { "id": id, "icon": buttonIcon(id), "text": Actions.text(id) } })
    //% "Stop selecting"
    out.push({ "id": "done", "icon": buttonIcon("done"), "text": qsTrId("lautta-dir-sel-done") })
    return out
}

// ---- Selecting by dragging (iOS Photos, Google Photos, Immich) ----
// Holding an item flips it; moving the finger on gives every item from the
// held one to the one under the finger, in reading order, the held item's
// new state. Moving back shrinks the range: the items it leaves get the
// state they had before the drag. A drag is { anchor, target, from, to }:
// the held item, the state it gives, and the range it covers.

// The drag once item `anchor`, `selected` or not, is held. It covers
// nothing yet: dragTo(drag, anchor) flips the held item.
function dragStart(anchor, selected) {
    return { "anchor": anchor, "target": !selected, "from": anchor, "to": anchor - 1 }
}

// The finger reached item `index`: { drag, flips }, the drag after it and
// the items whose state flips, in index order. `before(i)` is item i's
// state before the drag. Both ranges hold the anchor, so only their ends
// differ: an item entering the range flips unless it already had the
// target state, one leaving it flips back on the same condition.
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

// How far a drag scrolls the view per step with the finger at `y`, the
// items being shown between `top` and `bottom`: in a zone `zone` high at
// either edge towards that edge, faster the nearer the finger is to it, at
// most `fastest`; 0 elsewhere. Negative is up.
function autoScroll(y, top, bottom, zone, fastest) {
    if (y < top + zone)
        return -fastest * Math.min(1, (top + zone - y) / zone)
    if (y > bottom - zone)
        return fastest * Math.min(1, (y - bottom + zone) / zone)
    return 0
}
