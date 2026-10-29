// SPDX-License-Identifier: LGPL-2.1-or-later
// TextViewer's backdrop on the target (PRV-4, real Silica): the page has a
// backdrop under the text that darkens a dark ambience and lightens a light
// one (the page's palette is switched to a light ambience to see both), and
// the text reads on it over the worst wallpaper there can be: white behind
// a dark ambience's backdrop, black behind a light one's. How it looks,
// only a phone shows.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window

    // WCAG's contrast ratio wants at least this much for text.
    readonly property real readable: 4.5

    property bool scenarioRunning: true
    property int waited: 0
    property int step: 0
    property var page

    ViewerTools { id: tools }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function findAll(item, pred, out) {
        if (!item)
            return out
        if (pred(item) && out.indexOf(item) < 0)
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out)
        if (item.contentItem && item.contentItem !== item)
            findAll(item.contentItem, pred, out)
        return out
    }

    function indexIn(items, item) {
        for (var i = 0; i < items.length; ++i) {
            if (items[i] === item)
                return i
        }
        return -1
    }

    // Relative luminance, WCAG.
    function luminance(c) {
        function linear(v) { return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4) }
        return 0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
    }

    function contrast(a, b) {
        var high = Math.max(luminance(a), luminance(b))
        var low = Math.min(luminance(a), luminance(b))
        return (high + 0.05) / (low + 0.05)
    }

    // `top` laid over the opaque `ground`.
    function over(top, ground) {
        return Qt.rgba(top.r * top.a + ground.r * (1 - top.a),
                       top.g * top.a + ground.g * (1 - top.a),
                       top.b * top.a + ground.b * (1 - top.a), 1)
    }

    // The backdrop in the scheme the page's palette has now: lighter or
    // darker than the text, and the text reads on it over the worst wallpaper.
    function checkBackdrop(backdrop, dark) {
        var text = page.palette.primaryColor
        var worst = over(backdrop.color, dark ? Qt.rgba(1, 1, 1, 1) : Qt.rgba(0, 0, 0, 1))
        var name = dark ? "dark" : "light"
        if (backdrop.color.a < 0.5)
            return fail("the " + name + " backdrop is at least half opaque: " + backdrop.color)
        if (dark ? luminance(backdrop.color) >= luminance(text) : luminance(backdrop.color) <= luminance(text))
            return fail("the " + name + " backdrop is " + (dark ? "darker" : "lighter") + " than the text: " + backdrop.color + " and " + text)
        if (contrast(text, worst) < readable)
            return fail("the text reads on the " + name + " backdrop: " + contrast(text, worst) + " for " + text + " on " + worst)
        return true
    }

    Timer {
        interval: 100
        repeat: true
        running: window.scenarioRunning
        onTriggered: {
            if (++window.waited > 100)
                return window.fail("timed out")
            var view = window.findAll(window.page, function(x) { return x.forceLayout !== undefined }, [])[0]
            var backdrops = window.findAll(window.page, function(x) { return x.objectName === "textBackdrop" }, [])
            if (!view || backdrops.length === 0)
                return window.waited > 50 ? window.fail("the page has a list and a backdrop") : undefined
            var backdrop = backdrops[0]
            if (backdrops.length !== 1)
                return window.fail("one backdrop: " + backdrops.length)
            // Under the text: the whole page, first in its stacking.
            var siblings = window.page.children
            var order = window.indexIn(siblings, backdrop) - window.indexIn(siblings, view)
            if (backdrop.width !== window.page.width || backdrop.height !== window.page.height || backdrop.parent !== window.page)
                return window.fail("the backdrop covers the page: " + backdrop.width + "x" + backdrop.height)
            if (!backdrop.visible || backdrop.z > view.z || order > 0)
                return window.fail("the backdrop is under the text")
            var dark = window.page.palette.colorScheme === Theme.LightOnDark
            if (!window.checkBackdrop(backdrop, dark))
                return
            if (window.step++ === 0)
                window.page.palette.colorScheme = dark ? Theme.DarkOnLight : Theme.LightOnDark
            else
                window.scenarioRunning = false
        }
    }

    Component.onCompleted: {
        tools.installFixture(Qt.resolvedUrl("host/fixtures/notes-crlf.txt"), "lautta://user-documents/backdrop.txt")
        page = pageStack.push(pageComponent, {}, PageStackAction.Immediate)
    }

    Component {
        id: pageComponent
        TextViewer { uri: "lautta://user-documents/backdrop.txt" }
    }
}
