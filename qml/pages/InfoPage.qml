// SPDX-License-Identifier: LGPL-2.1-or-later
// Info of one file or folder (OPS-12): details, tags (ORG-3), checksums,
// favourite, file system and links.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/operations"
import "../components/operations/Json.js" as Json
import "../components/ErrorText.js" as ErrorText

Page {
    id: page

    property string uri

    readonly property var info: Json.value(infoModel.infoJson, {})
    readonly property var tags: Json.value(infoModel.tagsJson, [])
    property var checksums: ({})

    allowedOrientations: Orientation.All

    function hex(algo) {
        return checksums[algo] || ""
    }

    function typeText() {
        if (info.isDir)
            //% "Folder"
            return qsTrId("lautta-info-folder")
        if (info.isSymlink)
            //% "Symbolic link"
            return qsTrId("lautta-info-symlink")
        return info.mimeType || App.categoryOf(info.name || "")
    }

    function sizeText() {
        if (info.size === undefined || info.size === null)
            return ""
        //% "%1 (%2 bytes)"
        return qsTrId("lautta-info-size-bytes").arg(Format.formatFileSize(info.size)).arg(info.size)
    }

    function timeText(ms) {
        return ms === undefined || ms === null ? "" : Format.formatDate(new Date(ms), Formatter.TimepointRelative)
    }

    function spaceText() {
        if (info.freeBytes === undefined || info.freeBytes === null)
            return ""
        //% "%1 of %2"
        return qsTrId("lautta-info-space").arg(Format.formatFileSize(info.freeBytes)).arg(Format.formatFileSize(info.totalBytes || 0))
    }

    function pickLinkFolder(hard) {
        var picker = pageStack.push(Qt.resolvedUrl("../dialogs/FolderPickerDialog.qml"), {
            "title": hard
                     //% "Hard link in…"
                     ? qsTrId("lautta-info-hard-link-in")
                     //% "Symbolic link in…"
                     : qsTrId("lautta-info-symlink-in"),
            //% "Create link"
            "acceptText": qsTrId("lautta-info-create-link"),
            "startUri": info.parentUri || App.parentUri(page.uri)
        })
        picker.accepted.connect(function() { infoModel.makeLink(picker.selectedUri, hard) })
    }

    InfoModel {
        id: infoModel
        uri: page.uri
        onChecksumReady: {
            var copy = page.checksums
            copy[algo] = hex
            page.checksums = copy
        }
        onLinkMade: {
            //% "Link created"
            notice.text = qsTrId("lautta-info-link-made")
            notice.show()
        }
        onFailed: {
            notice.text = ErrorText.message(kind, { "item": info.name || "", "location": info.locationName || "" })
            notice.show()
        }
    }

    Notice {
        id: notice
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            PageHeader {
                title: page.info.name || App.nameOf(page.uri)
                description: page.info.folderName || ""
            }

            Item {
                width: parent.width
                height: Theme.itemSizeHuge

                OpIcon {
                    anchors.centerIn: parent
                    width: Theme.iconSizeLarge
                    height: Theme.iconSizeLarge
                    sourceSize.width: Theme.iconSizeLarge
                    sourceSize.height: Theme.iconSizeLarge
                    isDir: page.info.isDir === true
                    category: page.info.category || "file"
                }
            }

            DetailItem {
                //% "Type"
                label: qsTrId("lautta-info-type")
                value: page.typeText()
            }
            DetailItem {
                visible: page.info.isDir !== true && value.length > 0
                //% "Size"
                label: qsTrId("lautta-info-size")
                value: page.sizeText()
            }
            DetailItem {
                visible: value.length > 0
                //% "Modified"
                label: qsTrId("lautta-info-modified")
                value: page.timeText(page.info.modified)
            }
            DetailItem {
                visible: value.length > 0
                //% "Created"
                label: qsTrId("lautta-info-created")
                value: page.timeText(page.info.created)
            }
            DetailItem {
                visible: page.info.linkTarget !== undefined && page.info.linkTarget !== null
                //% "Points to"
                label: qsTrId("lautta-info-points-to")
                value: page.info.linkTarget || ""
            }
            BackgroundItem {
                width: parent.width
                height: locationItem.height
                onClicked: Clipboard.text = page.info.address || ""

                DetailItem {
                    id: locationItem
                    //% "Location"
                    label: qsTrId("lautta-info-location")
                    //% "%1 (Copy)"
                    value: qsTrId("lautta-info-copy-address").arg(page.info.address || "")
                }
            }
            DetailItem {
                //% "Permissions"
                label: qsTrId("lautta-info-permissions")
                value: page.info.modeText || ""
            }
            DetailItem {
                visible: value.length > 0
                //% "Owner"
                label: qsTrId("lautta-info-owner")
                value: page.info.owner || ""
            }

            SectionHeader {
                //% "Tags"
                text: qsTrId("lautta-info-tags")
            }
            Flow {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                spacing: Theme.paddingMedium
                layoutDirection: Qt.RightToLeft

                BackgroundItem {
                    width: addLabel.width + Theme.paddingLarge
                    height: Theme.itemSizeExtraSmall
                    onClicked: {
                        var dialog = pageStack.push(Qt.resolvedUrl("../dialogs/TagAssignDialog.qml"), {
                            "uris": [page.uri]
                        })
                        dialog.accepted.connect(function() { infoModel.reload() })
                    }

                    Label {
                        id: addLabel
                        anchors.centerIn: parent
                        color: Theme.highlightColor
                        font.pixelSize: Theme.fontSizeSmall
                        //% "+ Add"
                        text: qsTrId("lautta-info-tag-add")
                    }
                }
                Repeater {
                    model: page.tags

                    Rectangle {
                        width: tagRow.width + Theme.paddingLarge
                        height: Theme.itemSizeExtraSmall * 0.7
                        radius: height / 2
                        color: Theme.rgba(Theme.primaryColor, Theme.opacityFaint)

                        Row {
                            id: tagRow
                            anchors.centerIn: parent
                            spacing: Theme.paddingSmall

                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                width: Theme.paddingMedium
                                height: width
                                radius: width / 2
                                color: modelData.colour
                            }
                            Label {
                                text: modelData.name
                                font.pixelSize: Theme.fontSizeSmall
                            }
                        }
                    }
                }
            }

            SectionHeader {
                visible: page.info.canChecksum === true
                //% "Checksums"
                text: qsTrId("lautta-info-checksums")
            }
            Repeater {
                model: page.info.canChecksum === true ? ["sha256", "md5"] : []

                Column {
                    width: column.width

                    Label {
                        x: Theme.horizontalPageMargin
                        width: parent.width - 2 * Theme.horizontalPageMargin
                        color: Theme.secondaryHighlightColor
                        font.pixelSize: Theme.fontSizeExtraSmall
                        text: modelData === "sha256" ? "SHA-256" : "MD5"
                    }
                    Label {
                        x: Theme.horizontalPageMargin
                        width: parent.width - 2 * Theme.horizontalPageMargin
                        visible: page.hex(modelData).length > 0
                        text: page.hex(modelData)
                        font.family: "monospace"
                        font.pixelSize: Theme.fontSizeExtraSmall
                        wrapMode: Text.WrapAnywhere
                    }
                    Button {
                        anchors.right: parent.right
                        anchors.rightMargin: Theme.horizontalPageMargin
                        visible: page.hex(modelData).length === 0
                        enabled: !infoModel.checksumBusy
                        //% "Calculate"
                        text: qsTrId("lautta-info-calculate")
                        onClicked: infoModel.computeChecksum(modelData)
                    }
                }
            }

            SectionHeader {
                //% "Favourite"
                text: qsTrId("lautta-info-favourite")
            }
            Button {
                anchors.right: parent.right
                anchors.rightMargin: Theme.horizontalPageMargin
                //% "Add parent folder to Favourites"
                text: qsTrId("lautta-info-add-favourite")
                onClicked: pageStack.push(Qt.resolvedUrl("../dialogs/FavouriteDialog.qml"), {
                    "uri": page.info.parentUri || App.parentUri(page.uri)
                })
            }

            SectionHeader {
                visible: filesystemFree.visible || filesystemType.visible || linksRow.visible
                //% "Filesystem"
                text: qsTrId("lautta-info-filesystem")
            }
            DetailItem {
                id: filesystemFree
                visible: value.length > 0
                //% "Free space"
                label: qsTrId("lautta-info-free-space")
                value: page.spaceText()
            }
            DetailItem {
                id: filesystemType
                visible: value.length > 0
                //% "Filesystem"
                label: qsTrId("lautta-info-fs-type")
                value: page.info.fsType || ""
            }
            Column {
                id: linksRow
                width: parent.width
                visible: page.info.canHardlink === true || page.info.canSymlink === true

                Button {
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.horizontalPageMargin
                    visible: page.info.canHardlink === true && page.info.isDir !== true
                    //% "Hard link…"
                    text: qsTrId("lautta-info-hard-link")
                    onClicked: page.pickLinkFolder(true)
                }
                Button {
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.horizontalPageMargin
                    visible: page.info.canSymlink === true
                    //% "Symbolic link…"
                    text: qsTrId("lautta-info-symbolic-link")
                    onClicked: page.pickLinkFolder(false)
                }
            }
        }

        VerticalScrollDecorator { }
    }
}
