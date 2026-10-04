// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6

QtObject {
    Component.onCompleted: {
        if (1 + 1 !== 2)
            console.error("arithmetic is broken")
    }
}
