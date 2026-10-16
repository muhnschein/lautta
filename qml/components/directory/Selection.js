// SPDX-License-Identifier: LGPL-2.1-or-later
// When selection mode starts and ends (SPEC §15.3, board DirectorySelect).
// Picking the first item starts it; dropping the last one ends it, unless
// the mode was started with nothing picked yet (pulley Select), which waits
// for the first tap. The panel's dismiss button and the actions end it too.
.pragma library

// The panel actions after which the selection is done. Delete ends it once
// its remorse timer has run out, so an undo keeps the selection.
var endingActions = ["copy", "cut", "share", "compress"]

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
