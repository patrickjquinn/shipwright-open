// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Add or edit an entry. Saving an edit keeps the previous version in the
// entry's history (KeePass-compatible).

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0

Dialog {
    id: dialog
    objectName: "editDialog"

    // The entry as ShoalKeys.entry() returns it; empty for a new one.
    property var entry: ({})
    property string error: ""

    readonly property string otpError: ShoalKeys.checkOtp(otp.text)
    canAccept: (title.text.length > 0 || username.text.length > 0 || password.text.length > 0)
               && otpError.length === 0

    function lines(text) {
        return text.split("\n").map(function(s) { return s.trim() }).filter(function(s) { return s.length > 0 })
    }

    function toJson() {
        var fields = []
        for (var i = 0; i < fieldModel.count; ++i) {
            var f = fieldModel.get(i)
            if (f.name.trim().length > 0)
                fields.push({ name: f.name, value: f.value, "protected": f.secret })
        }
        return JSON.stringify({
            id: entry.id || "",
            title: title.text,
            username: username.text,
            password: password.text,
            urls: lines(urls.text),
            otp: otp.text,
            notes: notes.text,
            tags: tags.text.split(",").map(function(s) { return s.trim() }).filter(function(s) { return s.length > 0 }),
            group: group.text,
            fields: fields
        })
    }

    onAccepted: {
        var r = JSON.parse(ShoalKeys.saveEntry(toJson()))
        if (r.error)
            console.warn("Shoal Keys: save failed: " + r.error)
    }

    onAcceptBlocked: error = otpError.length ? otpError
                                             : qsTr("Fill in at least a title, user name or password.")

    Component.onCompleted: {
        var fs = entry.fields || []
        for (var i = 0; i < fs.length; ++i)
            fieldModel.append({ name: fs[i].name, value: fs[i].value, secret: fs[i]["protected"] })
    }

    ListModel { id: fieldModel }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                acceptText: qsTr("Save")
                title: dialog.entry.id ? qsTr("Edit entry") : qsTr("New entry")
            }

            Label {
                objectName: "editError"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                bottomPadding: Theme.paddingMedium
                wrapMode: Text.Wrap
                color: Theme.errorColor
                visible: dialog.error.length > 0
                text: dialog.error
            }

            TextField {
                id: title
                objectName: "titleField"
                width: parent.width
                label: qsTr("Title")
                placeholderText: qsTr("Title")
                text: dialog.entry.title || ""
                onTextChanged: dialog.error = ""
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: username.focus = true
            }
            TextField {
                id: username
                width: parent.width
                label: qsTr("User name")
                placeholderText: qsTr("User name")
                text: dialog.entry.username || ""
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: password.focus = true
            }
            PasswordField {
                id: password
                width: parent.width
                label: qsTr("Password")
                placeholderText: qsTr("Password")
                text: dialog.entry.password || ""
                description: text.length ? JSON.parse(ShoalKeys.strength(text)).label : ""
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: urls.focus = true
            }
            ValueButton {
                objectName: "generateButton"
                label: qsTr("Generate a password")
                onClicked: {
                    var d = pageStack.push(Qt.resolvedUrl("GeneratorDialog.qml"))
                    d.accepted.connect(function() { password.text = d.value })
                }
            }
            TextArea {
                id: urls
                width: parent.width
                label: qsTr("Websites")
                placeholderText: qsTr("Websites")
                description: qsTr("One address per line")
                text: (dialog.entry.urls || []).join("\n")
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
            }
            TextField {
                id: otp
                width: parent.width
                label: qsTr("One-time password")
                placeholderText: qsTr("One-time password")
                text: dialog.entry.otp || ""
                errorHighlight: dialog.otpError.length > 0
                description: dialog.otpError.length ? dialog.otpError : qsTr("Secret key or otpauth:// link")
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: notes.focus = true
            }
            TextArea {
                id: notes
                width: parent.width
                label: qsTr("Notes")
                placeholderText: qsTr("Notes")
                text: dialog.entry.notes || ""
            }
            TextField {
                id: tags
                width: parent.width
                label: qsTr("Tags")
                placeholderText: qsTr("Tags")
                description: qsTr("Separated by commas")
                text: (dialog.entry.tags || []).join(", ")
                EnterKey.iconSource: "image://theme/icon-m-enter-next"
                EnterKey.onClicked: group.focus = true
            }
            TextField {
                id: group
                width: parent.width
                label: qsTr("Group")
                placeholderText: qsTr("Group")
                description: qsTr("For example Internet/Mail")
                text: dialog.entry.group || ""
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }

            SectionHeader { text: qsTr("Fields") }
            Repeater {
                model: fieldModel
                Column {
                    id: fieldRow
                    required property var model
                    required property int index
                    width: column.width
                    TextField {
                        width: parent.width
                        label: qsTr("Field name")
                        placeholderText: qsTr("Field name")
                        text: fieldRow.model.name
                        onTextChanged: fieldModel.setProperty(fieldRow.index, "name", text)
                        EnterKey.iconSource: "image://theme/icon-m-enter-next"
                        EnterKey.onClicked: fieldValue.focus = true
                    }
                    TextField {
                        id: fieldValue
                        width: parent.width
                        label: qsTr("Value")
                        placeholderText: qsTr("Value")
                        text: fieldRow.model.value
                        echoMode: fieldRow.model.secret ? TextInput.Password : TextInput.Normal
                        onTextChanged: fieldModel.setProperty(fieldRow.index, "value", text)
                        EnterKey.iconSource: "image://theme/icon-m-enter-close"
                        EnterKey.onClicked: focus = false
                    }
                    TextSwitch {
                        text: qsTr("Hidden")
                        description: qsTr("Shown as dots until revealed")
                        checked: fieldRow.model.secret
                        onCheckedChanged: fieldModel.setProperty(fieldRow.index, "secret", checked)
                    }
                    ValueButton {
                        label: qsTr("Remove this field")
                        onClicked: fieldModel.remove(fieldRow.index)
                    }
                }
            }
            ValueButton {
                objectName: "addFieldButton"
                label: qsTr("Add a field")
                onClicked: fieldModel.append({ name: "", value: "", secret: false })
            }
        }
        VerticalScrollDecorator { }
    }
}
