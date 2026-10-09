// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// The generator's content, shared by GeneratorPage (tap the result to copy
// it) and GeneratorDialog (accept to use it in an entry): random passwords
// and EFF-wordlist passphrases, with an entropy estimate.

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0

Column {
    id: form

    // true: tapping the result copies it (GeneratorPage).
    property bool copyOnTap: true
    property var result: ({})
    readonly property string value: result.error ? "" : (result.value || "")
    property bool _copied: false

    function generate() {
        if (mode.currentIndex === 0)
            result = JSON.parse(ShoalKeys.generatePassword(length.value, lower.checked, upper.checked,
                                                      digits.checked, symbols.checked, ambiguous.checked))
        else
            result = JSON.parse(ShoalKeys.generatePassphrase(words.value, separator.text, capitalize.checked,
                                                        addDigit.checked))
    }

    width: parent ? parent.width : 0
    Component.onCompleted: generate()

    BackgroundItem {
        id: resultItem
        objectName: "resultItem"
        width: parent.width
        height: Math.max(Theme.itemSizeLarge, resultColumn.height + 2 * Theme.paddingLarge)
        enabled: form.copyOnTap && form.value.length > 0
        onClicked: {
            ShoalKeys.copy(form.value)
            form._copied = true
            copiedTimer.restart()
        }

        Timer {
            id: copiedTimer
            interval: 1500
            onTriggered: form._copied = false
        }

        Column {
            id: resultColumn
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.paddingSmall

            Label {
                objectName: "generated"
                width: parent.width
                // Between words where it can (a passphrase breaks at its
                // separators), anywhere in a password.
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignHCenter
                font.family: "monospace"
                font.pixelSize: Theme.fontSizeLarge
                color: form.result.error ? Theme.errorColor
                     : resultItem.highlighted || !form.copyOnTap ? Theme.highlightColor : Theme.primaryColor
                // Generated passwords contain `<`, `&` and friends.
                textFormat: Text.PlainText
                text: form.result.error || form.result.value || ""
            }
            Label {
                objectName: "strength"
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: resultItem.highlighted || !form.copyOnTap ? Theme.secondaryHighlightColor
                                                                 : Theme.secondaryColor
                visible: !form.result.error
                text: form._copied ? qsTr("Copied")
                    : form.copyOnTap ? qsTr("%1 bits · %2 · Tap to copy").arg(form.result.bits || 0).arg(form.result.label || "")
                    : qsTr("%1 bits · %2").arg(form.result.bits || 0).arg(form.result.label || "")
            }
        }
    }

    ComboBox {
        id: mode
        objectName: "kindCombo"
        label: qsTr("Kind")
        menu: ContextMenu {
            MenuItem { text: qsTr("Password") }
            MenuItem { text: qsTr("Passphrase") }
        }
        onCurrentIndexChanged: form.generate()
    }

    // ---- password ----
    Column {
        width: parent.width
        visible: mode.currentIndex === 0
        Slider {
            id: length
            width: parent.width
            minimumValue: 8
            maximumValue: 64
            stepSize: 1
            value: 20
            label: qsTr("Length")
            valueText: qsTr("%n character(s)", "", value)
            onValueChanged: form.generate()
        }
        TextSwitch { id: lower; text: qsTr("Lower case"); checked: true; onCheckedChanged: form.generate() }
        TextSwitch { id: upper; text: qsTr("Upper case"); checked: true; onCheckedChanged: form.generate() }
        TextSwitch { id: digits; text: qsTr("Digits"); checked: true; onCheckedChanged: form.generate() }
        TextSwitch { id: symbols; text: qsTr("Symbols"); checked: true; onCheckedChanged: form.generate() }
        TextSwitch {
            id: ambiguous
            text: qsTr("Avoid look-alike characters")
            description: qsTr("Leaves out I, l, 1, O, 0 and similar")
            onCheckedChanged: form.generate()
        }
    }

    // ---- passphrase ----
    Column {
        width: parent.width
        visible: mode.currentIndex === 1
        Slider {
            id: words
            width: parent.width
            minimumValue: 4
            maximumValue: 12
            stepSize: 1
            value: 6
            label: qsTr("Words")
            valueText: qsTr("%n word(s)", "", value)
            onValueChanged: form.generate()
        }
        TextField {
            id: separator
            width: parent.width
            label: qsTr("Separator")
            placeholderText: qsTr("Separator")
            text: "-"
            inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
            onTextChanged: form.generate()
            EnterKey.iconSource: "image://theme/icon-m-enter-close"
            EnterKey.onClicked: focus = false
        }
        TextSwitch { id: capitalize; text: qsTr("Capitalise words"); onCheckedChanged: form.generate() }
        TextSwitch {
            id: addDigit
            text: qsTr("Add a digit")
            description: qsTr("Words come from a list of 7,776 common English words")
            onCheckedChanged: form.generate()
        }
    }
}
