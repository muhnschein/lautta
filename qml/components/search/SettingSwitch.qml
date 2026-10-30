// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

TextSwitch {
    property string settingKey
    // Reading App.settingsJson makes binding follow every change.
    readonly property bool current: {
        var dependency = App.settingsJson
        return settingKey.length > 0 && App.setting(settingKey) === true
    }

    automaticCheck: false
    checked: current
    onClicked: App.setSetting(settingKey, JSON.stringify(!current))
}
