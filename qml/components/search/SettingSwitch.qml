// SPDX-License-Identifier: LGPL-2.1-or-later
// A TextSwitch bound to a boolean global setting (SPEC §18); persisted
// through App.setSetting.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0

TextSwitch {
    // Key of Settings::to_map, for example "folders_first".
    property string settingKey
    // Reading App.settingsJson makes the binding follow every change.
    readonly property bool current: {
        var dependency = App.settingsJson
        return settingKey.length > 0 && App.setting(settingKey) === true
    }

    automaticCheck: false
    checked: current
    onClicked: App.setSetting(settingKey, JSON.stringify(!current))
}
