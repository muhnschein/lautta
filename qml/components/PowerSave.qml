// SPDX-License-Identifier: LGPL-2.1-or-later
// Follows mce's battery saving into App: server thumbnails then come only from cache or EXIF.
// Sailjail's Base permission lets apps call get_psm_state and hear psm_state_ind.
import QtQuick 2.6
import Nemo.DBus 2.0
import Lautta 1.0

Item {
    DBusInterface {
        bus: DBus.SystemBus
        service: "com.nokia.mce"
        path: "/com/nokia/mce/signal"
        iface: "com.nokia.mce.signal"
        signalsEnabled: true

        function psm_state_ind(on) {
            App.setPowerSave(on)
        }
    }

    DBusInterface {
        id: request

        bus: DBus.SystemBus
        service: "com.nokia.mce"
        path: "/com/nokia/mce/request"
        iface: "com.nokia.mce.request"
    }

    Component.onCompleted: request.typedCall("get_psm_state", [], function (on) { App.setPowerSave(on) })
}
