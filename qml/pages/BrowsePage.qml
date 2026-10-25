// SPDX-License-Identifier: LGPL-2.1-or-later
// Browse: favourites, the device, volumes, servers and nearby servers
// (SPEC §15.2). Standalone it shows no trace of the bridge (UI-8): the
// Servers and Nearby sections and the bridge's pulley entries only exist
// while the bridge is usable.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../components"
import "../components/browse"
import "../components/transfers"
import "../components/ErrorText.js" as ErrorText
import "../components/Open.js" as Open
import "../components/browse/BrowseText.js" as BrowseText
import "../components/browse/RowStatus.js" as RowStatus

Page {
    id: page

    // The rows; tests give the page a ListModel with every kind of row.
    property var listModel: locations
    // netvfs opens Settings › Storage, which the sandbox cannot (LOC-3,
    // NVB-13); tests set it.
    property bool storageSettings: Bridge.storageSettings
    readonly property bool discovering: status === PageStatus.Active
                                        && Qt.application.state === Qt.ApplicationActive && Bridge.ready

    allowedOrientations: Orientation.All

    onDiscoveringChanged: Bridge.setDiscover(discovering)
    // Back in the foreground: refresh, which rescans the volumes (LOC-3) in
    // case the media watch missed a mount inside the sandbox.
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
            // A file favourite opens like a file in its folder (ORG-1).
            if (row.isFile)
                Open.open(pageStack, App, Operations, row.uri, false, "", "")
            else
                openDirectory(row.uri)
            break
        case "recents":
            pageStack.push(Qt.resolvedUrl("RecentsPage.qml"))
            break
        case "deleted":
            pageStack.push(Qt.resolvedUrl("TrashPage.qml"))
            break
        case "nearby":
        case "adhocRecent":
            connectTo(row.uri)
            break
        case "volume":
            // A drive that is not mounted is mounted, then opens (LOC-3).
            if (row.status === "unmounted")
                Bridge.mountDrive(row.drive, row.name)
            else
                openDirectory(row.uri)
            break
        case "account":
        case "adhoc":
            if (row.attention.length > 0 && row.attention !== "")
                pageStack.push(Qt.resolvedUrl("../components/browse/ServerAttentionPage.qml"), {
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

    function connectTo(url) {
        pageStack.push(Qt.resolvedUrl("../dialogs/ConnectServerDialog.qml"), { "url": url })
    }

    function openFavourite(uri) {
        pageStack.push(Qt.resolvedUrl("FavouritePage.qml"), { "uri": uri })
    }

    // Row remorses start here, not in the delegate: the Browse rows can be
    // rebuilt during the countdown, and a callback made in a delegate that
    // is gone can no longer reach the page's models.
    function remorseRow(item, text, action) {
        item.remorseAction(text, action, Number(App.setting("remorse_seconds")) * 1000)
    }

    function removeFavourite(item, id) {
        //% "Removing favourite"
        remorseRow(item, qsTrId("lautta-browse-removing-favourite"), function() { favourites.remove(id) })
    }

    function removeServer(item, id, connected) {
        //% "Removing server"
        remorseRow(item, qsTrId("lautta-browse-removing-server"), function() {
            if (connected)
                Bridge.forgetAdHoc(id)
            else
                Bridge.removeRecent(id)
        })
    }

    function copyAddress(text) {
        Clipboard.text = text
    }

    LocationsModel {
        id: locations
    }

    FavouritesModel {
        id: favourites
    }

    SilicaListView {
        id: list

        anchors.fill: parent
        model: page.listModel

        header: PageHeader {
            //% "Browse"
            title: qsTrId("lautta-browse-title")
        }

        section.property: "section"
        section.criteria: ViewSection.FullString
        section.delegate: SectionHeader {
            text: BrowseText.sectionTitle(section)
        }

        PullDownMenu {
            MenuItem {
                //% "Settings"
                text: qsTrId("lautta-browse-settings")
                onClicked: pageStack.push(Qt.resolvedUrl("SettingsPage.qml"))
            }
            MenuItem {
                visible: Bridge.ready
                //% "Add server"
                text: qsTrId("lautta-browse-add-server")
                onClicked: Bridge.addServer("")
            }
            MenuItem {
                text: Transfers.activeCount > 0
                      //% "Activity (%1)"
                      ? qsTrId("lautta-browse-transfers-count").arg(Transfers.activeCount)
                      //% "Activity"
                      : qsTrId("lautta-browse-transfers")
                onClicked: pageStack.push(Qt.resolvedUrl("TransfersPage.qml"))
            }
        }

        delegate: ListItem {
            id: row

            readonly property string kind: model.kind
            readonly property bool banner: kind === "consent" || kind === "unavailable"
            // A drive the bridge can mount, and the bridge's id of a
            // volume's drive (LOC-3).
            readonly property bool unmounted: kind === "volume" && model.status === "unmounted"
            readonly property string drive: model.drive || ""
            readonly property string subtitle: {
                switch (kind) {
                case "favourite":
                    return model.place
                case "volume":
                    return BrowseText.volumeLine(model.free, model.total, model.fs)
                case "nearby":
                    return BrowseText.nearbyLine(model.provider, model.host)
                default:
                    return ""
                }
            }
            // Only how the connection or mount stands, as an icon, no
            // protocol or host (NVB-4, LOC-3).
            readonly property string statusIndicator: RowStatus.indicator(kind, model.status, model.connection,
                                                                          model.connectionError, drive)
            readonly property bool hasMenu: kind === "favourite" || kind === "folder" || kind === "volume"
                                            || kind === "account" || kind === "adhoc" || kind === "adhocRecent"

            // The status icon sits on the right, so a server row is as
            // tall as a folder row (§15.2).
            contentHeight: banner ? bannerContent.height + 2 * Theme.paddingMedium
                                  : (subtitle.length > 0 ? Theme.itemSizeMedium : Theme.itemSizeSmall)
            menu: hasMenu ? contextMenu : null
            enabled: true

            onClicked: if (!banner) page.openRow(model)

            HighlightImage {
                visible: !row.banner
                opacity: row.unmounted ? Theme.opacityHigh : 1.0
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                source: model.icon
                width: Theme.iconSizeMedium
                height: width
                sourceSize.width: width
                sourceSize.height: height
                color: model.colour.length > 0 ? model.colour : Theme.primaryColor
                highlighted: row.highlighted
                highlightColor: Theme.highlightColor
            }

            Column {
                visible: !row.banner
                opacity: row.unmounted ? Theme.opacityHigh : 1.0
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin + Theme.iconSizeMedium + Theme.paddingLarge
                    right: statusIcon.indicator.length > 0 ? statusIcon.left : parent.right
                    rightMargin: statusIcon.indicator.length > 0 ? Theme.paddingLarge : Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }

                Label {
                    width: parent.width
                    text: BrowseText.rowName(model.kind, model.name)
                    color: row.highlighted ? Theme.highlightColor : Theme.primaryColor
                    truncationMode: TruncationMode.Fade
                }
                // A remote favourite's place has the server icon (§15.5).
                PlaceLabel {
                    visible: row.subtitle.length > 0
                    width: parent.width
                    text: row.subtitle
                    remote: row.kind === "favourite" && App.isRemote(model.uri)
                    color: row.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                }
            }

            // How a server's connection or a drive's mount stands; the name
            // gives way first.
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

            // The consent row (NVB-3) and the reconnecting notice (NVB-12).
            Row {
                id: bannerContent
                visible: row.banner
                x: Theme.horizontalPageMargin
                y: Theme.paddingMedium
                width: parent.width - 2 * Theme.horizontalPageMargin
                spacing: Theme.paddingMedium

                Column {
                    width: parent.width - actionButton.width - parent.spacing
                    anchors.verticalCenter: parent.verticalCenter

                    Label {
                        width: parent.width
                        wrapMode: Text.Wrap
                        text: row.kind === "consent"
                              //% "Allow access in the notification from netvfs"
                              ? qsTrId("lautta-browse-consent")
                              : ErrorText.message("BridgeUnavailable")
                        font.pixelSize: Theme.fontSizeSmall
                    }
                    Label {
                        visible: row.kind === "consent"
                        width: parent.width
                        wrapMode: Text.Wrap
                        //% "Network locations need your permission"
                        text: qsTrId("lautta-browse-consent-hint")
                        font.pixelSize: Theme.fontSizeExtraSmall
                        color: Theme.secondaryColor
                    }
                }
                Button {
                    id: actionButton
                    anchors.verticalCenter: parent.verticalCenter
                    preferredWidth: Theme.buttonWidthSmall
                    text: row.kind === "consent"
                          //% "Ask again"
                          ? qsTrId("lautta-browse-ask-again")
                          //% "Retry"
                          : qsTrId("lautta-browse-retry")
                    onClicked: {
                        if (row.kind === "consent")
                            Bridge.requestConsent()
                        else
                            Bridge.poke()
                    }
                }
            }

            Component {
                id: contextMenu

                ContextMenu {
                    // Favourites
                    MenuItem {
                        visible: row.kind === "favourite"
                        //% "Edit favourite"
                        text: qsTrId("lautta-browse-edit-favourite")
                        onClicked: page.openFavourite(model.uri)
                    }

                    // Servers (NVB-5)
                    MenuItem {
                        visible: row.kind === "adhocRecent"
                        //% "Connect"
                        text: qsTrId("lautta-browse-connect-recent")
                        onClicked: page.connectTo(model.uri)
                    }
                    MenuItem {
                        visible: row.kind === "account"
                        //% "Edit server"
                        text: qsTrId("lautta-browse-edit-server")
                        onClicked: Bridge.editAccount(model.uri)
                    }
                    MenuItem {
                        visible: row.kind === "account" && model.attention === "auth-failed"
                        //% "Update sign-in"
                        text: qsTrId("lautta-browse-update-sign-in")
                        onClicked: Bridge.editAccount(model.uri)
                    }
                    MenuItem {
                        visible: row.kind === "account" && model.attention === "server-identity-changed"
                        //% "Review server identity"
                        text: qsTrId("lautta-browse-review-identity")
                        onClicked: Bridge.editAccount(model.uri)
                    }

                    // Folders and volumes (ORG-1, LOC-3)
                    MenuItem {
                        visible: row.unmounted
                        //% "Mount"
                        text: qsTrId("lautta-browse-mount")
                        onClicked: Bridge.mountDrive(row.drive, model.name)
                    }
                    MenuItem {
                        visible: (row.kind === "folder" || row.kind === "volume") && !row.unmounted
                        //% "Add to favourites"
                        text: qsTrId("lautta-browse-add-favourite")
                        onClicked: page.openFavourite(model.uri)
                    }
                    // Every row with a menu, favourites too, but a drive
                    // that is not mounted has no address yet.
                    MenuItem {
                        visible: !row.unmounted
                        //% "Copy address"
                        text: qsTrId("lautta-browse-copy-address")
                        onClicked: page.copyAddress(row.kind === "adhocRecent" ? model.place : App.displayAddress(model.uri))
                    }
                    MenuItem {
                        visible: row.kind === "volume" && !row.unmounted && row.drive.length > 0
                        //% "Unmount"
                        text: qsTrId("lautta-browse-unmount")
                        onClicked: Bridge.unmountDrive(row.drive, model.name)
                    }
                    MenuItem {
                        visible: row.kind === "volume" && page.storageSettings
                        //% "Open Storage settings"
                        text: qsTrId("lautta-browse-open-storage-settings")
                        onClicked: Bridge.openStorageSettings()
                    }
                    MenuItem {
                        visible: row.kind === "favourite"
                        //% "Remove from favourites"
                        text: qsTrId("lautta-browse-remove-favourite")
                        onClicked: page.removeFavourite(row, parseInt(model.itemId))
                    }
                    MenuItem {
                        visible: row.kind === "adhoc" || row.kind === "adhocRecent"
                        //% "Remove from recents"
                        text: qsTrId("lautta-browse-remove-recent")
                        onClicked: page.removeServer(row, model.itemId, row.kind === "adhoc")
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
