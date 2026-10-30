// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Nemo.Configuration 1.0
import Lautta 1.0
import "../components"
import "../components/directory"
import "../components/ErrorText.js" as ErrorText
import "../components/FileIcons.js" as FileIcons
import "../components/operations/Json.js" as Json
import "../components/operations/Share.js" as Share

// Folder view inside the dialog, changed in place: a back swipe cancels a dialog. Empty
// `destUri` = Locations.
Dialog {
    id: page

    property var resources: []
    property var entries: []
    property string destUri
    property alias recentJson: recentStore.value
    readonly property var recent: Share.usable(Json.value(recentStore.value, []),
                                               function (uri) { return App.locationName(uri).length > 0 })
    readonly property var skippedNames: entries.filter(function (e) { return !e.readable })
                                               .map(function (e) { return e.name })
    readonly property int readableCount: entries.length - skippedNames.length
    readonly property string note: Share.note(readableCount, skippedNames)
    //% "Save here"
    readonly property string acceptText: qsTrId("lautta-share-accept")

    function pathOf(resource) {
        if (typeof resource === "string")
            return resource
        return resource.filePath || resource.url || ""
    }

    function load() {
        var paths = resources.map(pathOf)
        var probed = JSON.parse(Operations.probeShared(JSON.stringify(paths)))
        for (var i = 0; i < probed.length; ++i)
            if (paths[i].length === 0)
                probed[i].readable = false
        entries = probed
        destUri = recent.length > 0 ? recent[0] : downloads()
    }

    function downloads() {
        var all = JSON.parse(Operations.destinationsJson())
        for (var i = 0; i < all.length; ++i)
            if (all[i].uri.indexOf("user-downloads") >= 0)
                return all[i].uri
        return all.length > 0 ? all[0].uri : ""
    }

    allowedOrientations: Orientation.All
    canAccept: readableCount > 0 && destUri.length > 0 && folder.model.writable && !folder.model.loading
               && folder.model.errorKind === ""

    onAccepted: {
        var uris = entries.filter(function (e) { return e.readable }).map(function (e) { return e.uri })
        recentStore.value = JSON.stringify(Share.remember(recent, destUri))
        Operations.copyTo(JSON.stringify(uris), destUri)
    }

    Component.onCompleted: load()

    ConfigurationValue {
        id: recentStore
        key: "/apps/harbour-lautta/share/recent"
        defaultValue: "[]"
    }

    DirectoryView {
        id: folder

        objectName: "shareFolder"
        anchors.fill: parent
        visible: page.destUri.length > 0
        uri: page.destUri
        picking: true
        pickAcceptText: page.acceptText
        pickNote: page.note
        pickNoteWarns: page.skippedNames.length > 0
        onOpenFolder: page.destUri = uri
        onGoTo: page.destUri = uri
    }

    SilicaListView {
        id: locations

        objectName: "shareLocations"
        anchors.fill: parent
        visible: page.destUri.length === 0
        model: visible ? roots : null

        PickerRootsModel {
            id: roots
        }

        header: Column {
            width: locations.width

            DialogHeader {
                acceptText: page.acceptText
            }
            PageHeader {
                //% "Locations"
                title: qsTrId("lautta-share-locations")
            }
            Label {
                visible: text.length > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignRight
                wrapMode: Text.Wrap
                text: page.note
                color: page.skippedNames.length > 0 ? Theme.errorColor : Theme.secondaryHighlightColor
                font.pixelSize: Theme.fontSizeExtraSmall
            }

            SectionHeader {
                visible: page.recent.length > 0
                //% "Saved here lately"
                text: qsTrId("lautta-share-recent")
            }

            Repeater {
                model: page.recent

                BackgroundItem {
                    width: locations.width
                    height: Theme.itemSizeMedium
                    onClicked: page.destUri = modelData

                    AppIcon {
                        id: clock
                        x: Theme.horizontalPageMargin
                        anchors.verticalCenter: parent.verticalCenter
                        icon: "image://theme/icon-m-history"
                        highlighted: parent.highlighted
                    }
                    Column {
                        anchors {
                            left: clock.right
                            leftMargin: Theme.paddingLarge
                            right: parent.right
                            rightMargin: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }

                        Label {
                            width: parent.width
                            text: App.nameOf(modelData)
                            truncationMode: TruncationMode.Fade
                            color: parent.parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                        }
                        Label {
                            visible: text.length > 0
                            width: parent.width
                            text: Operations.displayPath(App.parentUri(modelData))
                            truncationMode: TruncationMode.Fade
                            font.pixelSize: Theme.fontSizeExtraSmall
                            color: parent.parent.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                        }
                    }
                }
            }

            SectionHeader {
                text: qsTrId("lautta-share-locations")
            }
        }

        delegate: BackgroundItem {
            width: locations.width
            height: Theme.itemSizeMedium
            enabled: model.selectable
            opacity: enabled ? 1 : Theme.opacityLow
            onClicked: page.destUri = model.uri

            AppIcon {
                id: glyph
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                icon: FileIcons.sized(Share.locationIcon(model.kind), iconSize)
                highlighted: parent.highlighted
            }
            Column {
                anchors {
                    left: glyph.right
                    leftMargin: Theme.paddingLarge
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }

                Label {
                    width: parent.width
                    text: model.name
                    truncationMode: TruncationMode.Fade
                    color: parent.parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                Label {
                    visible: model.status !== "ready"
                    width: parent.width
                    text: model.attention.length > 0 ? model.attention
                                                     : ErrorText.message("NetworkUnreachable", { "location": model.name })
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    truncationMode: TruncationMode.Fade
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
