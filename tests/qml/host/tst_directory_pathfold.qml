// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/directory/PathFold.js" as PathFold

Item {
    readonly property var deep: ["Documents", "Uni", "Thesis", "Chapters", "3 Methods", "Figures", "Source", "Run 12"]

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    // Fits up to `chars` characters, as a monospace font would.
    function fold(names, chars) {
        return PathFold.fold(names, function (text) { return text.length <= chars })
    }

    Component.onCompleted: {
        same(fold([], 0), "", "nothing above a location")
        same(fold(["Documents"], 3), "Documents", "one name never folds")
        same(fold(["Documents", "Uni"], 3), "Documents › Uni", "location and parent never fold")
        same(fold(deep, 200), deep.join(" › "), "all of it when it fits")
        var exact = deep.join(" › ").length
        same(fold(deep, exact), deep.join(" › "), "fits to the character")
        same(fold(deep, exact - 1), "Documents › … › Thesis › Chapters › 3 Methods › Figures › Source › Run 12",
             "folds the folder after the location first")
        same(fold(deep, 31), "Documents › … › Source › Run 12", "as many of the nearest as fit")
        same(fold(deep, 30), "Documents › … › Run 12", "location and parent")
        same(fold(deep, 3), "Documents › … › Run 12", "and no less: the label fades the rest")
        same(fold(["A", "B", "C"], 3), "A › … › C", "three names: the middle one folds")

        same(PathFold.columns([1, 2, 3, 4, 5], 1), [[1, 2, 3, 4, 5]], "one column")
        same(PathFold.columns([1, 2, 3, 4, 5], 2), [[1, 2, 3], [4, 5]], "two columns, read down then across")
        same(PathFold.columns([1, 2, 3, 4], 2), [[1, 2], [3, 4]], "even split")
        same(PathFold.columns([1], 2), [[1]], "too few for two")
        same(PathFold.columns([], 2), [], "none")
        same(PathFold.columns([1, 2], 0), [[1, 2]], "no columns asked: one")
    }
}
