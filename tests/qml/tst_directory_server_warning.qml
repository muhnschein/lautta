// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: win
    property bool reachedEnd

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
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

    function texts(buttons) {
        return buttons.map(function (b) { return b.text + (b.primary ? "*" : "") + ":" + b.action }).join(", ")
    }

    function check(view) {
        var name = view.model.locationName
        expect(view.placeholderTitle("error", "ServerIdentityChanged") === qsTrId("lautta-attention-identity-title"),
               "identity title as on the attention page")
        expect(view.placeholderTitle("error", "AuthFailed") === qsTrId("lautta-attention-auth-title"),
               "sign-in title as on the attention page")
        expect(view.placeholderHint("error", "ServerIdentityChanged")
               === qsTrId("lautta-err-identity-changed").arg(name) + "\n\n" + qsTrId("lautta-attention-identity-blocked"),
               "identity hint as on the attention page: " + view.placeholderHint("error", "ServerIdentityChanged"))

        var want = qsTrId("lautta-attention-review") + "*:account, " + qsTrId("lautta-dir-details") + ":details"
        var got = texts(view.placeholderButtons("error", "ServerIdentityChanged"))
        expect(got === want, "identity change: review first, no retry: " + got)
        want = qsTrId("lautta-attention-update") + "*:account, " + qsTrId("lautta-dir-details") + ":details"
        got = texts(view.placeholderButtons("error", "AuthFailed"))
        expect(got === want, "sign-in failure: update first: " + got)
        want = qsTrId("lautta-dir-retry") + ":retry, " + qsTrId("lautta-dir-details") + ":details"
        got = texts(view.placeholderButtons("error", "NotFound"))
        expect(got === want, "other errors keep retry: " + got)
    }

    Component.onCompleted: {
        pageStack.push(pageComponent, { "uri": "lautta://user-documents/" }, PageStackAction.Immediate)
        var views = findAll(pageStack.currentPage, function (x) { return x.placeholderButtons !== undefined }, [])
        if (expect(views.length === 1, "one folder view: " + views.length))
            check(views[0])
        reachedEnd = true
    }

    Component {
        id: pageComponent

        DirectoryPage { }
    }
}
