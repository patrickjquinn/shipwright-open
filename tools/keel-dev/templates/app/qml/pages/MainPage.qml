// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT-0

// The first page: a list of greetings from the Rust `Greeter` singleton,
// with a pull-down menu to add one or start over.

import QtQuick 2.0
import Sailfish.Silica 1.0
import {{qml_uri}} 1.0
// Silica-only Screen members (sizeCategory, topCutout, Screen.Large, ...):
// write them qualified, with `import Sailfish.Silica 1.0 as S` and
// `S.Screen.sizeCategory`. An unqualified Screen is the first import's that
// has one, Qt Quick's once this file imports QtQuick without a 2.x version.

Page {
    objectName: "mainPage"
    allowedOrientations: Orientation.All

    SilicaListView {
        id: list
        anchors.fill: parent
        model: Greeter.count

        header: PageHeader {
            title: "{{title}}"
            description: Greeter.summary
        }

        PullDownMenu {
            MenuItem {
                text: "Start over"
                enabled: Greeter.count > 0
                onClicked: Greeter.reset()
            }
            MenuItem {
                text: "Say hello"
                onClicked: Greeter.increment()
            }
        }

        delegate: ListItem {
            contentHeight: Theme.itemSizeSmall
            Label {
                anchors {
                    left: parent.left
                    right: parent.right
                    leftMargin: Theme.horizontalPageMargin
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                text: Greeter.line(index)
                color: highlighted ? Theme.highlightColor : Theme.primaryColor
            }
        }

        ViewPlaceholder {
            enabled: list.count === 0
            text: "No greetings yet"
            hintText: "Pull down to say hello"
        }

        VerticalScrollDecorator { }
    }
}
