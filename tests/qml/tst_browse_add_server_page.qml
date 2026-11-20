// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    id: test

    property bool scenarioRunning: true
    property bool reachedEnd

    function check(page) {
        var list = page.listView
        list.forceLayout()
        var got = []
        for (var i = 0; i < list.count; ++i)
            got.push(list.model.get(i).provider + "=" + list.model.get(i).name)
        var want = "sftp=SFTP,smb=SMB,webdav=WebDAV,ftps=FTPS"
        if (got.join(",") !== want)
            console.error("FAILED: Add server offers " + got.join(",") + ", not " + want)
        else
            reachedEnd = true
        scenarioRunning = false
    }

    initialPage: Component {
        AddServerPage {
            id: page
            Component.onCompleted: test.check(page)
        }
    }
}
