// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// The generator as a picker, from the edit dialog: accept to use the
// generated password (`value`), pull down for a new one.

import QtQuick
import Sailfish.Silica 1.0

Dialog {
    id: dialog
    objectName: "generatorDialog"

    readonly property string value: form.value

    canAccept: value.length > 0

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PullDownMenu {
            MenuItem {
                objectName: "generateItem"
                text: qsTr("Generate again")
                onClicked: form.generate()
            }
        }

        Column {
            id: column
            width: parent.width

            DialogHeader {
                acceptText: qsTr("Use")
                title: qsTr("Generate a password")
            }

            GeneratorForm {
                id: form
                copyOnTap: false
            }
        }
        VerticalScrollDecorator { }
    }
}
