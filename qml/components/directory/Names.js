// SPDX-License-Identifier: LGPL-2.1-or-later
// Names the New folder and Rename dialogs refuse (SPEC §15.3).
.pragma library

function invalid(name) {
    return name.indexOf("/") >= 0 || name === "." || name === ".."
}
