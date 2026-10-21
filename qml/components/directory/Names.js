// SPDX-License-Identifier: LGPL-2.1-or-later
// Names the New folder and Rename dialogs refuse (SPEC §15.3).
.pragma library

// At the root of a local location this folder is the Recently deleted bin
// (OPS-8). The listing leaves it out, so the dialogs' clash check never sees
// it: a new folder of that name would vanish into the bin, a rename would
// fail or move the item there.
var binName = ".lautta-trash"

// `binHere`: the name would sit at the root of a local location.
function invalid(name, binHere) {
    return name.indexOf("/") >= 0 || name === "." || name === ".." || (binHere && name === binName)
}
