// SPDX-License-Identifier: LGPL-2.1-or-later Headless: no render loop, so looks (colours on
// screen, layout) stay device-proven; this checks what each warning level shows and does.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/components/home"

ApplicationWindow {
    id: test
    property bool reachedEnd

    ViewerTools { id: tools }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        return expect(done(), "timed out waiting for " + what)
    }

    function findAll(item, pred, out, seen) {
        seen = seen || []
        if (!item || seen.indexOf(item) >= 0)
            return out
        seen.push(item)
        if (pred(item))
            out.push(item)
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i)
            findAll(kids[i], pred, out, seen)
        if (item.contentItem)
            findAll(item.contentItem, pred, out, seen)
        return out
    }

    function named(root, name) {
        return findAll(root, function (x) { return x.objectName === name }, [])
    }

    function one(root, name) {
        var all = named(root, name)
        return expect(all.length === 1, "one " + name + ": " + all.length) ? all[0] : null
    }

    function hintOf(warning) {
        var labels = findAll(warning, function (x) { return x.wrapMode !== undefined && x.objectName === "" }, [])
        return labels.length === 1 ? String(labels[0].text) : "<" + labels.length + " hint labels>"
    }

    function open(attention) {
        pageStack.push(pageComponent, { "attention": attention }, PageStackAction.Immediate)
        return pageStack.currentPage
    }

    function checkIdentity() {
        var page = open("server-identity-changed")
        var warning = one(page, "attentionWarning")
        if (!warning)
            return
        expect(warning.level === "security", "identity change is a security warning: " + warning.level)
        var icon = one(warning, "placeholderIcon")
        expect(icon && icon.visible && String(icon.source) === "image://theme/icon-l-attention",
               "identity change shows the attention icon")
        expect(icon && Qt.colorEqual(icon.color, Theme.errorColor), "its icon is in the error colour")
        var kicker = one(warning, "placeholderKicker")
        expect(kicker && kicker.visible, "identity change says security warning")
        var title = one(warning, "placeholderTitle")
        expect(title && title.text === qsTrId("lautta-attention-identity-title"), "identity title: " + (title && title.text))
        expect(title && Qt.colorEqual(title.color, Theme.errorColor) && title.opacity === 1.0,
               "identity title is in the error colour, full strength")
        var hint = hintOf(warning)
        expect(hint === qsTrId("lautta-err-identity-changed").arg("NAS") + "\n\n" + qsTrId("lautta-attention-identity-blocked"),
               "identity hint names the server, the risk and that Lautta won't connect: " + hint)
        var buttons = named(warning, "placeholderButton")
        expect(buttons.length === 2, "two buttons: " + buttons.length)
        if (buttons.length === 2) {
            expect(buttons[0].text === qsTrId("lautta-attention-review"), "fix first: " + buttons[0].text)
            expect(buttons[1].text === qsTrId("lautta-attention-back"), "way out second: " + buttons[1].text)
            expect(!Qt.colorEqual(buttons[0].backgroundColor, buttons[1].backgroundColor), "the fix stands out")
        }
        page.act("account")
        expect(pageStack.currentPage === page && !pageStack.busy && pageStack.depth === 2,
               "review keeps the page; Settings opens over it")
        page.act("back")
        waitFor(function () { return pageStack.currentPage !== page && !pageStack.busy }, "go back to leave the page")
    }

    function checkSignIn() {
        var page = open("auth-failed")
        var warning = one(page, "attentionWarning")
        if (!warning)
            return
        expect(warning.level === "warning", "sign-in failure is a plain warning: " + warning.level)
        var icon = one(warning, "placeholderIcon")
        expect(icon && icon.visible && Qt.colorEqual(icon.color, Theme.highlightColor),
               "sign-in failure shows the attention icon in the highlight colour")
        var kicker = one(warning, "placeholderKicker")
        expect(kicker && !kicker.visible, "sign-in failure is not called a security warning")
        var title = one(warning, "placeholderTitle")
        expect(title && title.text === qsTrId("lautta-attention-auth-title"), "sign-in title: " + (title && title.text))
        expect(title && Qt.colorEqual(title.color, Theme.highlightColor), "sign-in title is in the highlight colour")
        expect(hintOf(warning) === qsTrId("lautta-err-auth-failed").arg("NAS"), "sign-in hint names the server: " + hintOf(warning))
        var buttons = named(warning, "placeholderButton")
        expect(buttons.length === 2 && buttons[0].text === qsTrId("lautta-attention-update")
               && buttons[1].text === qsTrId("lautta-attention-back"), "update sign-in, then go back")
        pageStack.pop(null, PageStackAction.Immediate)
    }

    Component.onCompleted: {
        if (pageStack.depth === 0)
            pageStack.push(blank, {}, PageStackAction.Immediate)
        checkIdentity()
        checkSignIn()
        reachedEnd = true
    }

    Component {
        id: blank

        Page { }
    }

    Component {
        id: pageComponent

        ServerAttentionPage {
            locationId: "nv-account:1"
            serverName: "NAS"
        }
    }
}
