// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// The password generator on its own (from the main page's pull-down menu):
// tap the result to copy it, pull down for a new one.

import QtQuick
import Sailfish.Silica 1.0

Page {
    id: page
    objectName: "generatorPage"

    property alias result: form.result

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

            PageHeader { title: qsTr("Password generator") }

            GeneratorForm {
                id: form
                copyOnTap: true
            }
        }
        VerticalScrollDecorator { }
    }
}
