// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the names New folder and Rename refuse (Names.js), SPEC
// §15.3: "/", "." and "..".
import QtQuick 2.6
import "../../../qml/components/directory/Names.js" as Names

Item {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    Component.onCompleted: {
        var bad = ["a/b", "/", ".", ".."]
        for (var i = 0; i < bad.length; ++i)
            check(Names.invalid(bad[i]), "invalid: " + bad[i])
        var good = ["lautta-trash", ".trash", "Plans", ".hidden", "..."]
        for (var j = 0; j < good.length; ++j)
            check(!Names.invalid(good[j]), "a free name: " + good[j])
    }
}
