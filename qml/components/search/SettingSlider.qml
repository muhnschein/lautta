// SPDX-License-Identifier: LGPL-2.1-or-later
// A Slider bound to a numeric global setting (SPEC §18); the value is
// written when the finger lifts.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

Slider {
    id: slider

    property string settingKey
    // Turns the value into the text above the slider, "5 s".
    property var valueFormat: function (n) { return String(n) }
    readonly property real current: {
        var dependency = App.settingsJson
        var v = settingKey.length > 0 ? App.setting(settingKey) : undefined
        return typeof v === "number" ? v : minimumValue
    }

    width: parent ? parent.width : 0
    stepSize: 1
    value: current
    valueText: valueFormat(value)
    onReleased: App.setSetting(settingKey, JSON.stringify(value))
}
