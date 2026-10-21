// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the names New folder and Rename refuse (Names.js), SPEC
// §15.3: no "/", "." or "..", and at a local location's root not the
// Recently deleted bin (OPS-8), which the listing hides.
import QtQuick 2.6
import "../../../qml/components/directory/Names.js" as Names

Item {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function testInvalid() {
        var bad = ["a/b", "/", ".", ".."]
        for (var i = 0; i < bad.length; ++i) {
            check(Names.invalid(bad[i], false), "invalid anywhere: " + bad[i])
            check(Names.invalid(bad[i], true), "invalid at a root: " + bad[i])
        }
    }

    function testBin() {
        check(Names.binName === ".lautta-trash", "the bin's name: " + Names.binName)
        check(Names.invalid(".lautta-trash", true), "the bin's name is taken at a local root")
        check(!Names.invalid(".lautta-trash", false), "below the root it is a name like any other")
        var good = [".lautta-trash2", "lautta-trash", ".trash", "Plans", ".hidden", "..."]
        for (var i = 0; i < good.length; ++i)
            check(!Names.invalid(good[i], true), "a free name at a root: " + good[i])
    }

    Component.onCompleted: {
        testInvalid()
        testBin()
    }
}
