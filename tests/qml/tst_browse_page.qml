// SPDX-License-Identifier: LGPL-2.1-or-later
// Every kind of Browse row, so that all delegates are instantiated.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {
    initialPage: Component {
        BrowsePage {
            listModel: ListModel {
        ListElement { section: "favourites"; uri: "lautta://user-documents/Uni"; name: "Thesis"; kind: "favourite"; icon: "image://theme/icon-m-favorite"; status: "ready"; attention: ""; colour: "#e7a33c"; count: -1; itemId: "1"; place: "Documents › Uni"; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "device"; uri: "lautta://user-documents/"; name: "Documents"; kind: "folder"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; colour: ""; count: 214; itemId: "user-documents"; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "device"; uri: ""; name: ""; kind: "deleted"; icon: "image://theme/icon-m-delete"; status: "ready"; attention: ""; colour: ""; count: 8; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "android"; uri: "lautta://android-dcim/"; name: "DCIM"; kind: "folder"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; colour: ""; count: 3410; itemId: "android-dcim"; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "volumes"; uri: "lautta://vol-SD/"; name: "SD card"; kind: "volume"; icon: "image://theme/icon-m-sd-card"; status: "ready"; attention: ""; colour: ""; count: -1; itemId: "vol-SD"; place: ""; provider: ""; host: ""; free: 62500000000; total: 128000000000; fs: "exFAT" }
        ListElement { section: "servers"; uri: ""; name: ""; kind: "consent"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; colour: ""; count: -1; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: ""; name: ""; kind: "unavailable"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; colour: ""; count: -1; itemId: ""; place: ""; provider: ""; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:1/"; name: "NAS"; kind: "account"; icon: "image://theme/icon-m-computer"; status: "ready"; attention: ""; colour: ""; count: -1; itemId: "nv-account:1"; place: ""; provider: "SFTP"; host: "nas.home"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:2/"; name: "Office"; kind: "account"; icon: "image://theme/icon-m-file-folder"; status: "attention"; attention: "auth-failed"; colour: ""; count: -1; itemId: "nv-account:2"; place: ""; provider: "WebDAV"; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:4/"; name: "Vault"; kind: "account"; icon: "image://theme/icon-m-file-folder"; status: "attention"; attention: "server-identity-changed"; colour: ""; count: -1; itemId: "nv-account:4"; place: ""; provider: "SFTP"; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-account:3/"; name: "Cabin"; kind: "account"; icon: "image://theme/icon-m-file-folder"; status: "connecting"; attention: ""; colour: ""; count: -1; itemId: "nv-account:3"; place: ""; provider: "SMB"; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "lautta://nv-adhoc:1/"; name: "host.example"; kind: "adhoc"; icon: "image://theme/icon-m-file-folder"; status: "ready"; attention: ""; colour: ""; count: -1; itemId: "nv-adhoc:1"; place: ""; provider: "SFTP"; host: "host.example"; free: -1; total: -1; fs: "" }
        ListElement { section: "servers"; uri: "ftps://ftp.kotisivu.fi/"; name: "ftp.kotisivu.fi"; kind: "adhocRecent"; icon: "image://theme/icon-m-file-folder"; status: "offline"; attention: ""; colour: ""; count: -1; itemId: "nv-adhoc:9"; place: "ftps://ftp.kotisivu.fi/"; provider: "FTPS"; host: ""; free: -1; total: -1; fs: "" }
        ListElement { section: "nearby"; uri: "sftp://raspberrypi.local/"; name: "raspberrypi"; kind: "nearby"; icon: "image://theme/icon-m-wlan"; status: "ready"; attention: ""; colour: ""; count: -1; itemId: "0"; place: ""; provider: "SFTP"; host: "raspberrypi.local"; free: -1; total: -1; fs: "" }
            }
        }
    }
}
