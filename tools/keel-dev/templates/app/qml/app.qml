// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT-0

// {{title}}: the main QML file, loaded by SailfishApp::pathToMainQml()
// from /usr/share/{{name}}/qml/{{name}}.qml. Only types Keel implements
// (keel/silica/COMPATIBILITY.md); `keel run` checks with keel-compat.

import QtQuick 2.0
import Sailfish.Silica 1.0

ApplicationWindow {
    initialPage: Qt.resolvedUrl("pages/MainPage.qml")
    cover: Qt.resolvedUrl("cover/CoverPage.qml")
    allowedOrientations: defaultAllowedOrientations
}
