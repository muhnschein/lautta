// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/home"
import "../components/transfers"
import "../components/viewers"
import "../components/ErrorText.js" as ErrorText
import "../components/FileIcons.js" as FileIcons
import "../components/Open.js" as Open
import "../components/home/HomeText.js" as HomeText
import "../components/home/RowStatus.js" as RowStatus

Page {
    id: page

    property var listModel: locations
    // Tests set it.
    property var recentsStore: recents
    // Sandbox cannot open Settings › Storage; netvfs can. Tests set it.
    property bool storageSettings: Bridge.storageSettings
    property real deviceFree: locations.deviceFree
    property real deviceTotal: locations.deviceTotal
    readonly property string coverKind: "home"
    readonly property bool discovering: status === PageStatus.Active
                                        && Qt.application.state === Qt.ApplicationActive && Bridge.ready

    allowedOrientations: Orientation.All

    onDiscoveringChanged: Bridge.setDiscover(discovering)
    // Rescans volumes: media watch can miss a mount inside sandbox.
    readonly property bool foreground: Qt.application.state === Qt.ApplicationActive
    onForegroundChanged: if (foreground && status === PageStatus.Active) locations.refresh()
    onStatusChanged: {
        if (status !== PageStatus.Active)
            return
        locations.refresh()
        favourites.reload()
    }
    Component.onCompleted: {
        locations.refresh()
        favourites.reload()
    }
    Component.onDestruction: Bridge.setDiscover(false)

    function openDirectory(uri) {
        pageStack.push(Qt.resolvedUrl("DirectoryPage.qml"), { "uri": uri })
    }

    function openRow(row) {
        switch (row.kind) {
        case "favourite":
            if (row.isFile)
                Open.open(pageStack, App, Operations, row.uri, false, "", "")
            else
                openDirectory(row.uri)
            break
        case "deleted":
            pageStack.push(Qt.resolvedUrl("TrashPage.qml"))
            break
        case "nearby":
            pageStack.push(Qt.resolvedUrl("../dialogs/NearbySetupDialog.qml"),
                           { "provider": row.uri.split("://")[0] })
            break
        case "volume":
            if (row.status === "locked") {
                if (storageSettings)
                    Bridge.openStorageSettings()
            } else if (row.status === "unmounted") {
                Bridge.mountDrive(row.drive, row.name)
            } else {
                openDirectory(row.uri)
            }
            break
        case "account":
            if (row.attention.length > 0 && row.attention !== "")
                pageStack.push(Qt.resolvedUrl("../components/home/ServerAttentionPage.qml"), {
                                   "locationId": row.itemId,
                                   "serverName": row.name,
                                   "attention": row.attention
                               })
            else
                openDirectory(row.uri)
            break
        default:
            openDirectory(row.uri)
            break
        }
    }

    function openAllRecents() {
        pageStack.push(Qt.resolvedUrl("RecentsPage.qml"))
    }

    // As SCR-38 Recents: a transferred file opens its folder.
    function openRecent(tile, tileSize) {
        if (tile.kind === "transferred")
            openDirectory(App.parentUri(tile.uri))
        else
            Open.open(pageStack, App, Operations, tile.uri, false, "", "", tileSize)
    }

    function openFavourite(uri) {
        pageStack.push(Qt.resolvedUrl("FavouritePage.qml"), { "uri": uri })
    }

    // Row remorses start here: rows can be rebuilt during countdown, and a callback from a gone
    // delegate cannot reach page models.
    function remorseRow(item, text, action) {
        item.remorseAction(text, action, Number(App.setting("remorse_seconds")) * 1000)
    }

    function removeFavourite(item, id) {
        //% "Removing favourite"
        remorseRow(item, qsTrId("lautta-home-removing-favourite"), function() { favourites.remove(id) })
    }

    LocationsModel {
        id: locations
    }

    FavouritesModel {
        id: favourites
    }

    RecentsModel {
        id: recents
    }

    ViewerPrefetch { id: prefetch }

    SilicaListView {
        id: list

        anchors.fill: parent
        model: page.listModel

        header: PageHeader {
            //% "Home"
            title: qsTrId("lautta-home-title")
        }

        section.property: "section"
        section.criteria: ViewSection.FullString
        section.delegate: Item {
            width: list.width
            height: sectionHeader.height

            SectionHeader {
                id: sectionHeader

                text: HomeText.sectionTitle(section)
            }
            Label {
                objectName: "deviceSpace"
                visible: section === "device" && text.length > 0
                anchors.baseline: sectionHeader.baseline
                x: Theme.horizontalPageMargin
                width: sectionHeader.width - sectionHeader.contentWidth - Theme.paddingLarge
                truncationMode: TruncationMode.Fade
                text: HomeText.deviceLine(page.deviceFree, page.deviceTotal)
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryColor
            }
        }

        PullDownMenu {
            MenuItem {
                //% "Settings"
                text: qsTrId("lautta-home-settings")
                onClicked: pageStack.push(Qt.resolvedUrl("SettingsPage.qml"))
            }
            MenuItem {
                visible: Bridge.ready
                //% "Add server"
                text: qsTrId("lautta-home-add-server")
                onClicked: Bridge.addServer("")
            }
            MenuItem {
                text: Transfers.activeCount > 0
                      //% "Activity (%1)"
                      ? qsTrId("lautta-home-transfers-count").arg(Transfers.activeCount)
                      //% "Activity"
                      : qsTrId("lautta-home-transfers")
                onClicked: pageStack.push(Qt.resolvedUrl("TransfersPage.qml"))
            }
        }

        delegate: ListItem {
            id: row

            readonly property string kind: model.kind
            readonly property bool banner: kind === "consent" || kind === "unavailable"
            readonly property bool strip: kind === "recent-strip"
            readonly property bool plain: !banner && !strip
            readonly property bool unmounted: kind === "volume" && model.status === "unmounted"
            readonly property bool locked: kind === "volume" && model.status === "locked"
            readonly property bool notMounted: unmounted || locked
            readonly property string drive: model.drive || ""
            // Tile whose menu is open.
            property int heldId: -1
            readonly property string subtitle: {
                switch (kind) {
                case "favourite":
                    return model.place
                case "volume":
                    return HomeText.volumeLine(model.free, model.total, model.fs)
                case "nearby":
                    return HomeText.nearbyLine(model.provider, model.host)
                default:
                    return ""
                }
            }
            readonly property string statusIndicator: RowStatus.indicator(kind, model.status, model.connection,
                                                                          model.connectionError, drive,
                                                                          Bridge.busyDrives)
            readonly property bool hasMenu: kind === "favourite" || kind === "folder"
                                            || (kind === "volume" && (!locked || page.storageSettings))
                                            || kind === "account"

            contentHeight: strip ? stripLoader.height
                                 : banner ? bannerContent.height + 2 * Theme.paddingMedium
                                          : (subtitle.length > 0 ? Theme.itemSizeMedium : Theme.itemSizeSmall)
            menu: strip ? tileMenu : (hasMenu ? contextMenu : null)
            enabled: true

            onPressed: if (kind === "favourite" && model.isFile) prefetch.start(model.uri, "")
            onClicked: if (plain) page.openRow(model)
            onPressAndHold: prefetch.stop()
            onCanceled: prefetch.stop()

            Loader {
                id: stripLoader

                width: parent.width
                active: row.strip
                sourceComponent: Component {
                    RecentStrip {
                        tiles: JSON.parse(model.tiles)
                        heldId: row.menuOpen ? row.heldId : -1
                        onTouched: if (tile.kind !== "transferred") prefetch.start(tile.uri, tile.mimeType)
                        onLetGo: prefetch.stop()
                        onOpened: page.openRecent(tile, tileSize)
                        onHeld: {
                            prefetch.stop()
                            row.heldId = tile.id
                            row.openMenu()
                        }
                        onAllOpened: page.openAllRecents()
                    }
                }
            }

            AppIcon {
                visible: row.plain
                opacity: row.notMounted ? Theme.opacityHigh : 1.0
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                icon: FileIcons.sized(model.icon, iconSize)
                color: model.colour.length > 0 ? model.colour : Theme.primaryColor
                highlighted: row.highlighted
            }

            Column {
                visible: row.plain
                opacity: row.notMounted ? Theme.opacityHigh : 1.0
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin + Theme.iconSizeMedium + Theme.paddingLarge
                    right: statusIcon.indicator.length > 0 ? statusIcon.left : parent.right
                    rightMargin: statusIcon.indicator.length > 0 ? Theme.paddingLarge : Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }

                Label {
                    width: parent.width
                    text: HomeText.rowName(model.kind, model.name)
                    color: row.highlighted ? Theme.highlightColor : Theme.primaryColor
                    truncationMode: TruncationMode.Fade
                }
                PlaceLabel {
                    visible: row.subtitle.length > 0
                    width: parent.width
                    text: row.subtitle
                    remote: row.kind === "favourite" && App.isRemote(model.uri)
                    color: row.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                }
            }

            RowStatusIcon {
                id: statusIcon

                indicator: row.statusIndicator
                highlighted: row.highlighted
                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
            }

            Row {
                id: bannerContent
                visible: row.banner
                x: Theme.horizontalPageMargin
                y: Theme.paddingMedium
                width: parent.width - 2 * Theme.horizontalPageMargin
                spacing: Theme.paddingMedium

                Label {
                    width: parent.width - actionButton.width - parent.spacing
                    anchors.verticalCenter: parent.verticalCenter
                    wrapMode: Text.Wrap
                    text: row.kind === "consent"
                          //% "Network locations need your permission"
                          ? qsTrId("lautta-home-consent-needed")
                          : ErrorText.message("BridgeUnavailable")
                    font.pixelSize: Theme.fontSizeSmall
                }
                Button {
                    id: actionButton
                    anchors.verticalCenter: parent.verticalCenter
                    preferredWidth: Theme.buttonWidthSmall
                    text: row.kind === "consent"
                          //% "Ask again"
                          ? qsTrId("lautta-home-ask-again")
                          //% "Retry"
                          : qsTrId("lautta-home-retry")
                    onClicked: {
                        if (row.kind === "consent")
                            Bridge.requestConsent()
                        else
                            Bridge.poke()
                    }
                }
            }

            Component {
                id: tileMenu

                ContextMenu {
                    MenuItem {
                        //% "Remove from recents"
                        text: qsTrId("lautta-recents-remove")
                        onClicked: page.recentsStore.remove(row.heldId)
                    }
                }
            }

            Component {
                id: contextMenu

                ContextMenu {
                    MenuItem {
                        visible: row.kind === "favourite"
                        //% "Edit favourite"
                        text: qsTrId("lautta-home-edit-favourite")
                        onClicked: page.openFavourite(model.uri)
                    }

                    MenuItem {
                        visible: row.kind === "account"
                        //% "Edit server"
                        text: qsTrId("lautta-home-edit-server")
                        onClicked: Bridge.editAccount(model.uri)
                    }
                    MenuItem {
                        visible: row.kind === "account" && model.attention === "auth-failed"
                        //% "Update sign-in"
                        text: qsTrId("lautta-home-update-sign-in")
                        onClicked: Bridge.editAccount(model.uri)
                    }
                    MenuItem {
                        visible: row.kind === "account" && model.attention === "server-identity-changed"
                        //% "Review server identity"
                        text: qsTrId("lautta-home-review-identity")
                        onClicked: Bridge.editAccount(model.uri)
                    }

                    MenuItem {
                        visible: row.unmounted
                        //% "Mount"
                        text: qsTrId("lautta-home-mount")
                        onClicked: Bridge.mountDrive(row.drive, model.name)
                    }
                    MenuItem {
                        visible: (row.kind === "folder" || row.kind === "volume") && !row.notMounted
                        //% "Add to favourites"
                        text: qsTrId("lautta-home-add-favourite")
                        onClicked: page.openFavourite(model.uri)
                    }
                    MenuItem {
                        visible: row.kind === "volume" && !row.notMounted && row.drive.length > 0
                        //% "Unmount"
                        text: qsTrId("lautta-home-unmount")
                        onClicked: Bridge.unmountDrive(row.drive, model.name)
                    }
                    MenuItem {
                        visible: row.kind === "volume" && page.storageSettings
                        //% "Open Storage settings"
                        text: qsTrId("lautta-home-open-storage-settings")
                        onClicked: Bridge.openStorageSettings()
                    }
                    MenuItem {
                        visible: row.kind === "favourite"
                        //% "Remove from favourites"
                        text: qsTrId("lautta-home-remove-favourite")
                        onClicked: page.removeFavourite(row, parseInt(model.itemId))
                    }
                }
            }
        }

        footer: Item {
            width: list.width
            height: activityBar.space
        }

        VerticalScrollDecorator { }
    }

    ActivityBar {
        id: activityBar
    }
}
