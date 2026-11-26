// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: test
    property bool reachedEnd

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function walk(item, out) {
        out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            walk(kids[i], out)
        if (item.contentItem && item.contentItem !== item)
            walk(item.contentItem, out)
        return out
    }

    function check(page) {
        var items = walk(page, [])
        var named = function (name) {
            return items.filter(function (x) { return x.objectName === name })[0]
        }
        var icon = named("aboutIcon")
        expect(icon && String(icon.source).indexOf("image://lautta-icon/") === 0
               && String(icon.source).indexOf("/icons/lautta.svg?") > 0 && icon.status === Image.Ready,
               "Lautta's glyph: " + (icon ? icon.source + " " + icon.status : "none"))
        expect(icon && Qt.colorEqual(icon.color, Theme.highlightColor), "in the ambience colour: " + (icon ? icon.color : "none"))
        var texts = items.filter(function (x) { return typeof x.text === "string" && x.visible })
                         .map(function (x) { return x.text })
        expect(texts.indexOf(qsTrId("lautta-about-tagline")) >= 0, "the claim: " + JSON.stringify(texts))
        expect(texts.indexOf(qsTrId("lautta-about-permissions")) < 0 && texts.indexOf("Permissions") < 0,
               "no Permissions section: " + JSON.stringify(texts))
        var labels = items.filter(function (x) { return typeof x.label === "string" && typeof x.value === "string" })
                          .map(function (x) { return x.label })
        var want = [qsTrId("lautta-about-status"), qsTrId("lautta-about-licence"), qsTrId("lautta-about-source")]
        expect(JSON.stringify(labels) === JSON.stringify(want), "Network locations shows the status only, no access: " + JSON.stringify(labels))
        expect(named("accessHint") === undefined, "and no hint to change access")
        expect(named("sourceCode") !== undefined, "Source code is a row to tap")
    }

    initialPage: Component {
        AboutPage {
            onStatusChanged: {
                if (status === PageStatus.Active && !test.reachedEnd) {
                    test.check(this)
                    test.reachedEnd = true
                }
            }
        }
    }
}
