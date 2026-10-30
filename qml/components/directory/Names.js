// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

function invalid(name) {
    return name.indexOf("/") >= 0 || name === "." || name === ".."
}
