// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

ComboBox {
    id: box

    property string settingKey
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
