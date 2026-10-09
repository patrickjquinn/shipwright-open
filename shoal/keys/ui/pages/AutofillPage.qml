// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Another app asks for a login (to fill a sign-in form) or to save one.
// The site is named large; nothing is sent until the person taps a login
// (or Save). Going back declines. See ../../app/src/autofill.rs.

pragma ComponentBehavior: Bound

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0

Page {
    id: page
    objectName: "autofillPage"

    property int request: 0
    readonly property var info: {
        var _ = ShoalKeys.revision + (ShoalKeys.busy ? 1 : 0)
        return JSON.parse(ShoalKeys.autofillRequest(request))
    }
    readonly property bool saving: info.kind === "save"
    property bool answered: false
    property string error: ""

    function answer(entryId) {
        error = ShoalKeys.autofillAnswer(request, entryId)
        if (error === "") {
            answered = true
            pageStack.pop()
        }
    }

    // Leaving without an answer declines.
    onStatusChanged: {
        if (status === PageStatus.Deactivating && !answered) {
            answered = true
            ShoalKeys.autofillDecline(request)
        }
    }

    SilicaListView {
        id: list
        anchors.fill: parent
        model: page.saving ? [] : (page.info.logins || [])

        header: Column {
            width: list.width
            spacing: Theme.paddingMedium
            PageHeader {
                title: page.saving ? (page.info.update ? qsTr("Update login?") : qsTr("Save login?"))
                                   : qsTr("Fill a login")
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                text: page.info.host || ""
                font.pixelSize: Theme.fontSizeLarge
                color: Theme.highlightColor
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                textFormat: Text.PlainText
                text: page.saving
                      ? (page.info.update
                         ? qsTr("Pacific asks to store a new password for %1 on this site.").arg(page.info.username || "")
                         : qsTr("Pacific asks to save the login %1 for this site.").arg(page.info.username || ""))
                      : qsTr("Pacific asks for a login for this site. Tap one to fill it in, or go back to send nothing.")
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                visible: page.error !== ""
                wrapMode: Text.Wrap
                text: page.error
                textFormat: Text.PlainText
                color: Theme.errorColor
            }
            Button {
                objectName: "autofillSave"
                visible: page.saving
                anchors.horizontalCenter: parent.horizontalCenter
                text: page.info.update ? qsTr("Update") : qsTr("Save")
                onClicked: page.answer("")
            }
            Button {
                visible: page.saving
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Not now")
                onClicked: pageStack.pop()
            }
        }

        delegate: ListItem {
            id: item
            required property var modelData
            contentHeight: Theme.itemSizeMedium
            highlighted: down || item.modelData.id === page.info.preselected
            onClicked: page.answer(item.modelData.id)
            Column {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * x
                anchors.verticalCenter: parent.verticalCenter
                Label {
                    width: parent.width
                    text: item.modelData.username || qsTr("(no user name)")
                    truncationMode: TruncationMode.Fade
                    textFormat: Text.PlainText
                    color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
                }
                Label {
                    width: parent.width
                    text: item.modelData.title
                    truncationMode: TruncationMode.Fade
                    textFormat: Text.PlainText
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                }
            }
        }

        BusyIndicator {
            anchors.centerIn: parent
            size: BusyIndicatorSize.Large
            running: page.info.waiting === true
        }

        ViewPlaceholder {
            enabled: !page.saving && !page.info.waiting && (page.info.logins || []).length === 0
            text: qsTr("No login for this site")
            hintText: qsTr("Go back to send nothing.")
        }

        VerticalScrollDecorator {}
    }
}
