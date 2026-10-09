// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
// Phone numbers from the address book, for starting a Signal or Telegram chat
// through the bridge (BridgedChatPage.qml). Sailfish.Pickers has no contact
// picker, so this lists matrix.contacts.phoneBook, which the address book
// read for contact matching provides (src/contactsbridge.cpp).
import QtQuick 2.0
import Sailfish.Silica 1.0

Page {
    id: page

    allowedOrientations: Orientation.All

    property string filter: ""

    function matches(row) {
        if (page.filter.length === 0) {
            return true
        }
        if (row.name.toLowerCase().indexOf(page.filter.toLowerCase()) >= 0) {
            return true
        }
        var digits = page.filter.replace(/[^0-9]/g, "")
        return digits.length > 0 && row.number.replace(/[^0-9]/g, "").indexOf(digits) >= 0
    }

    function filtered() {
        var rows = matrix.contacts.phoneBook
        var shown = []
        for (var i = 0; i < rows.length; i++) {
            if (matches(rows[i])) {
                shown.push(rows[i])
            }
        }
        return shown
    }

    SilicaListView {
        id: list

        anchors.fill: parent
        model: page.filtered()

        header: Column {
            width: list.width

            PageHeader {
                title: qsTr("Phone contacts")
                description: qsTr("For Signal or Telegram")
            }

            SearchField {
                width: parent.width
                placeholderText: qsTr("Search contacts")
                inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                onTextChanged: page.filter = text.trim()
            }
        }

        delegate: ListItem {
            contentHeight: Theme.itemSizeMedium
            onClicked: pageStack.push(Qt.resolvedUrl("BridgedChatPage.qml"),
                                      { contactName: modelData.name,
                                        number: modelData.number })

            Column {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter

                Label {
                    width: parent.width
                    truncationMode: TruncationMode.Fade
                    textFormat: Text.PlainText
                    color: highlighted ? Theme.highlightColor : Theme.primaryColor
                    text: modelData.name
                }

                Label {
                    width: parent.width
                    truncationMode: TruncationMode.Fade
                    textFormat: Text.PlainText
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                    text: modelData.mobile ? qsTr("%1 · mobile").arg(modelData.number)
                                           : modelData.number
                }
            }
        }

        ViewPlaceholder {
            enabled: list.count === 0
            text: matrix.contacts.status === "reading"
                  ? qsTr("Reading the address book…")
                  : (page.filter.length > 0 ? qsTr("No match")
                                            : qsTr("No contacts with a phone number"))
            hintText: matrix.contacts.status === "unavailable"
                      ? qsTr("The address book could not be read.") : ""
        }

        VerticalScrollDecorator {}
    }
}
