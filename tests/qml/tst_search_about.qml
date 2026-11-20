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
        expect(icon && String(icon.source).indexOf("icons/lautta-256.png") >= 0 && icon.status === Image.Ready,
               "the app's own icon: " + (icon ? icon.source + " " + icon.status : "none"))
        var texts = items.filter(function (x) { return typeof x.text === "string" && x.visible })
                         .map(function (x) { return x.text })
        expect(texts.indexOf(qsTrId("lautta-about-tagline")) >= 0
               && qsTrId("lautta-about-tagline") === "A better file manager for Sailfish OS.",
               "the claim: " + qsTrId("lautta-about-tagline"))
        expect(texts.indexOf("Permissions") < 0 && texts.indexOf("Your folders, Removable media, Audio") < 0,
               "no Permissions section: " + JSON.stringify(texts))
        var hint = named("accessHint")
        expect(hint && hint.text.indexOf("Servers you add") < 0 && hint.text.indexOf("Apps using network locations") >= 0,
               "the access hint names where to change access: " + (hint ? hint.text : "none"))
        expect(named("sourceCode") !== undefined, "Source code is a row to tap")
    }

    initialPage: Component {
        AboutPage {
            Component.onCompleted: {
                test.check(this)
                test.reachedEnd = true
            }
        }
    }
}
