// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/directory"
import "../components/ErrorText.js" as ErrorText
import "../components/FileIcons.js" as FileIcons
import "../components/browse/BrowseText.js" as BrowseText
import "../components/operations/Share.js" as Share

Dialog {
    id: page

    property var resources: []
    property string folderUri
    property var entries: []
    property var recent: []

    readonly property var readable: Share.readable(entries)
    readonly property int readableCount: readable.length
    readonly property int skippedCount: entries.length - readableCount
    readonly property string saveText: readableCount === 1
                                       //% "Save %1"
                                       ? qsTrId("lautta-share-one").arg(readable[0].name)
                                       //% "Save %n files"
                                       : qsTrId("lautta-share-count", readableCount)
    readonly property string skippedText: {
        if (skippedCount === 0)
            return ""
        if (skippedCount === 1)
            //% "%1 can't be read, skipped"
            return qsTrId("lautta-share-skipped-one").arg(Share.skipped(entries)[0].name)
        //% "%n files can't be read, skipped"
        return qsTrId("lautta-share-skipped", skippedCount)
    }

    function load() {
        var shared = Share.paths(resources)
        entries = Share.probed(shared, JSON.parse(Operations.probeShared(JSON.stringify(shared))))
        var places = JSON.parse(Operations.sharePlacesJson())
        recent = places.recent
        folderUri = places.start
    }

    function rootStatus(status, attention, name) {
        if (attention === "auth-failed")
            return qsTrId("lautta-attention-auth-title")
        if (attention === "server-identity-changed")
            return qsTrId("lautta-attention-identity-title")
        return status === "offline" ? ErrorText.message("NetworkUnreachable", { "location": name }) : ""
    }

    allowedOrientations: Orientation.All
    canAccept: Share.canSave(readableCount, folderUri, view.model.writable, view.model.loading, view.model.errorKind)

    onAccepted: {
        Operations.copyTo(JSON.stringify(Share.uris(readable)), folderUri)
        App.setSetting("share_folders", JSON.stringify(Share.remembered(folderUri, JSON.parse(App.settingsJson).share_folders)))
    }

    Component.onCompleted: load()

    PickerRootsModel {
        id: roots
    }

    DirectoryView {
        id: view

        objectName: "shareView"
        anchors.fill: parent
        picking: true
        uri: page.folderUri
        //% "Locations"
        rootTitle: qsTrId("lautta-share-locations")
        onOpenFolder: page.folderUri = uri
        onAncestorPicked: page.folderUri = uri
        onHomePicked: page.folderUri = ""
    }

    Column {
        objectName: "shareLead"
        parent: view.leadArea
        width: parent.width

        DialogHeader {
            dialog: page
            //% "Save here"
            acceptText: qsTrId("lautta-share-accept")
        }

        SectionHeader {
            objectName: "shareCount"
            text: page.saveText
        }

        Item {
            objectName: "shareSkipped"
            visible: page.skippedCount > 0
            width: parent.width
            height: visible ? skippedLabel.height + Theme.paddingMedium : 0

            HighlightImage {
                anchors {
                    right: skippedLabel.left
                    rightMargin: Theme.paddingSmall
                    verticalCenter: skippedLabel.verticalCenter
                }
                source: "image://theme/icon-s-warning"
                color: Theme.errorColor
            }

            Label {
                id: skippedLabel

                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                }
                width: Math.min(implicitWidth, parent.width - 2 * Theme.horizontalPageMargin - Theme.iconSizeSmall
                                               - Theme.paddingSmall)
                text: page.skippedText
                color: Theme.errorColor
                font.pixelSize: Theme.fontSizeSmall
                truncationMode: TruncationMode.Fade
            }
        }
    }

    SilicaListView {
        id: places

        objectName: "sharePlaces"
        parent: view.placesArea
        anchors.fill: parent
        clip: true
        model: roots

        header: Column {
            width: places.width

            SectionHeader {
                objectName: "shareRecentTitle"
                visible: page.recent.length > 0
                //% "Saved here lately"
                text: qsTrId("lautta-share-recent")
            }

            Repeater {
                model: page.recent

                ListItem {
                    id: lately

                    objectName: "shareRecent"
                    contentHeight: Theme.itemSizeMedium
                    onClicked: page.folderUri = modelData.uri

                    AppIcon {
                        id: latelyIcon

                        anchors {
                            left: parent.left
                            leftMargin: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        icon: "image://theme/icon-m-history"
                        highlighted: lately.highlighted
                    }

                    Column {
                        anchors {
                            left: latelyIcon.right
                            leftMargin: Theme.paddingLarge
                            right: parent.right
                            rightMargin: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }

                        Label {
                            width: parent.width
                            text: modelData.name
                            color: lately.highlighted ? Theme.highlightColor : Theme.primaryColor
                            truncationMode: TruncationMode.Fade
                        }

                        PlaceLabel {
                            visible: text.length > 0
                            width: parent.width
                            text: modelData.place
                            remote: modelData.remote
                            color: lately.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                        }
                    }
                }
            }
        }

        section.property: "kind"
        section.delegate: SectionHeader {
            text: BrowseText.sectionTitle(Share.sectionOf(section))
        }

        delegate: ListItem {
            id: place

            readonly property string status: page.rootStatus(model.status, model.attention, model.name)

            objectName: "sharePlace"
            contentHeight: status.length > 0 ? Theme.itemSizeMedium : Theme.itemSizeSmall
            enabled: model.selectable && model.writable
            opacity: enabled ? 1 : Theme.opacityLow
            onClicked: page.folderUri = model.uri

            AppIcon {
                id: placeIcon

                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                icon: FileIcons.sized(model.icon, iconSize)
                highlighted: place.highlighted
            }

            Column {
                anchors {
                    left: placeIcon.right
                    leftMargin: Theme.paddingLarge
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }

                Label {
                    width: parent.width
                    text: model.name
                    color: place.highlighted ? Theme.highlightColor : Theme.primaryColor
                    truncationMode: TruncationMode.Fade
                }

                Label {
                    visible: place.status.length > 0
                    width: parent.width
                    text: place.status
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    truncationMode: TruncationMode.Fade
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
