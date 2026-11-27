// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

Item {
    id: queue

    property var pending: []

    function stack() {
        return __silica_applicationwindow_instance.pageStack
    }

    function push(url, props, pushed) {
        pending = pending.concat([{ "url": url, "props": props, "pushed": pushed }])
        next()
    }

    function next() {
        if (stack().busy)
            return
        var entry = pending[0]
        pending = pending.slice(1)
        var page = stack().push(entry.url, entry.props)
        if (entry.pushed)
            entry.pushed(page)
    }

    Connections {
        target: queue.pending.length > 0 ? queue.stack() : null
        onBusyChanged: queue.next()
    }
}
