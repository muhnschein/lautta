// SPDX-License-Identifier: LGPL-2.1-or-later
// Target test: CrashReportPage instantiates with real Silica and the real Lautta types.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/pages"

ApplicationWindow {

    initialPage: Component {
        CrashReportPage {
            report: "thread 'main' panicked at\ncrates/lautta-core/src/x.rs:1:1\n\nversion: 1.0.0"
        }
    }
}
