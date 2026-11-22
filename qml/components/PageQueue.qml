// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0

// PageStack drops a push made during a transition: pages pushed from signals wait here until it is idle.
Item {
    id: queue

    property var pending: []

    // Resolved when used: window publishes its instance once created.
    function stack() {
        return __silica_applicationwindow_instance.pageStack
    }

    // `pushed(page)`, if given, once the page is on the stack.
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
