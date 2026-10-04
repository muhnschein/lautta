// SPDX-License-Identifier: LGPL-2.1-or-later
// A ComboBox bound to a global setting with a fixed set of values
// (SPEC §18). Closed it shows the label and the chosen entry.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ComboBox {
    id: box

    property string settingKey
    // The values the setting can have and their labels, in the same order.
    property var values: []
    property var labels: []
    readonly property var current: {
        var dependency = App.settingsJson
        return settingKey.length > 0 ? App.setting(settingKey) : undefined
    }

    currentIndex: Math.max(0, values.indexOf(current))
    menu: ContextMenu {
        Repeater {
            model: box.labels

            MenuItem {
                text: modelData
                onClicked: App.setSetting(box.settingKey, JSON.stringify(box.values[index]))
            }
        }
    }
}
