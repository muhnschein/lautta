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

    // Simple preferences live in dconf (SPEC §18); the core gets them at start.
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

    // App-wide reactions owned by the areas (doc/QML-API.md).
    BrowseHandlers { }
    OperationsHandlers { }
    TransfersHandlers { }

    UndoBanner { }
}
