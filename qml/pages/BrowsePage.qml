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

Page {
    id: page

    // The page-stack attachment (swipe left: Transfers) is made once.
    property bool attached
    // The rows; tests give the page a ListModel with every kind of row.
    property var listModel: locations
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
        if (!attached) {
            attached = true
            pageStack.pushAttached(Qt.resolvedUrl("TransfersPage.qml"))
        }
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
            pageStack.push(Qt.resolvedUrl("RecentlyDeletedPage.qml"))
            break
        case "nearby":
        case "adhocRecent":
            connectTo(row.uri)
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
        pageStack.push(Qt.resolvedUrl("../dialogs/FavouriteDialog.qml"), { "uri": uri })
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
            readonly property bool isServer: kind === "account" || kind === "adhoc" || kind === "adhocRecent"
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
            readonly property string serverText: {
                if (kind === "adhocRecent")
                    return BrowseText.recentLine(model.provider)
                if (model.status === "attention")
                    return BrowseText.attentionText(model.attention) + (model.provider.length > 0 ? " · " + model.provider : "")
                if (model.status === "connecting")
                    return BrowseText.reconnectingLine(model.provider)
                return BrowseText.connectedLine(model.provider, model.host)
            }
            readonly property bool hasMenu: kind === "favourite" || kind === "folder" || kind === "volume"
                                            || kind === "account" || kind === "adhoc" || kind === "adhocRecent"

            contentHeight: banner ? bannerContent.height + 2 * Theme.paddingMedium
                                  : (subtitle.length > 0 || isServer ? Theme.itemSizeMedium : Theme.itemSizeSmall)
            menu: hasMenu ? contextMenu : null
            enabled: true

            onClicked: if (!banner) page.openRow(model)

            HighlightImage {
                visible: !row.banner
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
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin + Theme.iconSizeMedium + Theme.paddingLarge
                    right: countLabel.visible ? countLabel.left : parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }

                Label {
                    width: parent.width
                    text: BrowseText.rowName(model.kind, model.name)
                    color: row.highlighted ? Theme.highlightColor : Theme.primaryColor
                    truncationMode: TruncationMode.Fade
                }
                Label {
                    visible: row.subtitle.length > 0
                    width: parent.width
                    text: row.subtitle
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: row.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                    truncationMode: TruncationMode.Fade
                }
                Row {
                    visible: row.isServer
                    width: parent.width
                    spacing: Theme.paddingMedium

                    Rectangle {
                        visible: model.status === "ready" || row.kind === "adhocRecent"
                        anchors.verticalCenter: parent.verticalCenter
                        width: Theme.paddingMedium
                        height: width
                        radius: width / 2
                        color: row.kind === "adhocRecent" ? "transparent" : Theme.highlightColor
                        border.width: row.kind === "adhocRecent" ? 1 : 0
                        border.color: Theme.secondaryColor
                    }
                    HighlightImage {
                        visible: model.status === "attention"
                        anchors.verticalCenter: parent.verticalCenter
                        source: "image://theme/icon-s-warning"
                        width: Theme.iconSizeExtraSmall
                        height: width
                        color: Theme.primaryColor
                        highlighted: row.highlighted
                    }
                    BusyIndicator {
                        visible: model.status === "connecting"
                        running: visible
                        anchors.verticalCenter: parent.verticalCenter
                        size: BusyIndicatorSize.ExtraSmall
                    }
                    Label {
                        width: parent.width - x
                        text: row.serverText
                        font.pixelSize: Theme.fontSizeExtraSmall
                        color: model.status === "attention" ? Theme.errorColor
                               : (row.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor)
                        truncationMode: TruncationMode.Fade
                    }
                }
            }

            Label {
                id: countLabel
                visible: !row.banner && model.count >= 0
                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                text: model.count
                font.pixelSize: Theme.fontSizeSmall
                color: row.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
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
                        visible: row.kind === "folder" || row.kind === "volume"
                        //% "Add to favourites"
                        text: qsTrId("lautta-browse-add-favourite")
                        onClicked: page.openFavourite(model.uri)
                    }
                    // Every row with a menu, favourites too.
                    MenuItem {
                        //% "Copy address"
                        text: qsTrId("lautta-browse-copy-address")
                        onClicked: page.copyAddress(row.kind === "adhocRecent" ? model.place : App.displayAddress(model.uri))
                    }
                    MenuItem {
                        visible: row.kind === "volume"
                        //% "Open Storage settings"
                        text: qsTrId("lautta-browse-open-storage-settings")
                        // The hand-off to the platform's Storage page is verified on device (SPEC §24.1).
                        onClicked: Qt.openUrlExternally("settings://system/storage")
                    }
                    MenuItem {
                        visible: row.kind === "folder" || row.kind === "volume" || row.isServer
                        //% "Hide from Browse"
                        text: qsTrId("lautta-browse-hide")
                        onClicked: {
                            //% "%1 hidden. Settings › Browse page shows it again."
                            hiddenNotice.text = qsTrId("lautta-browse-hidden").arg(model.name)
                            hiddenNotice.show()
                            locations.hide(model.section, model.kind, model.itemId)
                        }
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

    Notice {
        id: hiddenNotice
        duration: Notice.Long
    }
}
