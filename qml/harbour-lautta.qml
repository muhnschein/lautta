// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Nemo.Configuration 1.0
import Lautta 1.0
import "components"

ApplicationWindow {
    id: window

    initialPage: App.ready ? Qt.resolvedUrl("pages/BrowsePage.qml") : Qt.resolvedUrl("pages/StartErrorPage.qml")
    cover: Qt.resolvedUrl("cover/CoverPage.qml")
    allowedOrientations: defaultAllowedOrientations
    // Plain text everywhere: names, paths, server prompts, errors are untrusted. Silica default is
    // auto rich text. Coloured labels set StyledText of escaped text themselves.
    _defaultLabelFormat: Text.PlainText

    ConfigurationValue {
        id: settingsStore
        key: "/apps/harbour-lautta/settings"
        defaultValue: "{}"
    }

    Connections {
        target: App
        onSettingsJsonChanged: settingsStore.value = App.settingsJson
    }

    Component.onCompleted: {
        if (App.ready)
            App.loadSettings(settingsStore.value)
    }

    BrowseHandlers { }
    OperationsHandlers { }
    TransfersHandlers { }
}
