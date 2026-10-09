// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Import from files in Downloads (KDBX, Bitwarden JSON, 1Password 1PUX or
// CSV, Chrome or Firefox CSV), export a portable KDBX copy, and save the
// recovery key file.

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0

Page {
    id: page
    objectName: "importExportPage"

    // Filled by a scan on a worker thread (it opens every file in
    // Downloads); null until the first result arrives.
    property var candidates: null
    readonly property bool scanning: candidates === null

    function rescan() { ShoalKeys.scanImports() }

    Component.onCompleted: rescan()
    property string message: ""
    property bool messageIsError: false

    Connections {
        target: ShoalKeys
        function onFinished(op, ok, message) {
            if (op !== "import" && op !== "export")
                return
            page.message = message
            page.messageIsError = !ok
            if (op === "export")
                page.rescan()
        }
        function onImportCandidates(json) {
            page.candidates = JSON.parse(json)
        }
    }

    // The password field is shown only when a file needs one (KeePass).
    readonly property bool hasKeePass: (candidates || []).some(function(c) { return c.needsPassword === true })

    SilicaFlickable {
        id: flick
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PullDownMenu {
            // Hidden while a search runs (its busy indicator is on the page).
            visible: !page.scanning
            MenuItem {
                objectName: "rescanItem"
                text: qsTr("Look for files again")
                onClicked: {
                    page.candidates = null
                    page.rescan()
                }
            }
        }

        Column {
            id: column
            width: parent.width

            PageHeader { title: qsTr("Import and export") }

            Label {
                objectName: "importMessage"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                bottomPadding: Theme.paddingMedium
                wrapMode: Text.Wrap
                visible: text.length > 0
                color: page.messageIsError ? Theme.errorColor : Theme.highlightColor
                textFormat: Text.PlainText
                text: page.message
            }
            BusyIndicator {
                anchors.horizontalCenter: parent.horizontalCenter
                size: BusyIndicatorSize.Medium
                running: ShoalKeys.busy
                visible: running
            }

            SectionHeader { text: qsTr("Import from Downloads") }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                bottomPadding: Theme.paddingMedium
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: page.scanning
                      ? qsTr("Looking for exports in Downloads…")
                      : page.candidates.length
                        ? qsTr("Tap a file to add its entries. Entries already in the vault are skipped.")
                        : qsTr("No files to import in Downloads. Export your passwords from your old manager, such as KeePass, Bitwarden, 1Password, Chrome or Firefox, and put the file in Downloads.")
            }
            BusyIndicator {
                objectName: "scanIndicator"
                anchors.horizontalCenter: parent.horizontalCenter
                size: BusyIndicatorSize.Medium
                running: page.scanning
                visible: running
            }
            PasswordField {
                id: importPassword
                width: parent.width
                visible: page.hasKeePass
                label: qsTr("Password of the KeePass file")
                placeholderText: qsTr("Password of the KeePass file")
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }
            Repeater {
                model: page.candidates || []
                ListItem {
                    id: candidate
                    required property var modelData
                    width: column.width
                    contentHeight: Theme.itemSizeMedium
                    enabled: !ShoalKeys.busy
                    onClicked: {
                        page.message = ""
                        ShoalKeys.importFile(modelData.path, importPassword.text)
                    }
                    Icon {
                        id: fileIcon
                        x: Theme.horizontalPageMargin
                        anchors.verticalCenter: parent.verticalCenter
                        source: "image://theme/icon-m-document"
                    }
                    Column {
                        anchors {
                            left: fileIcon.right
                            leftMargin: Theme.paddingMedium
                            right: parent.right
                            rightMargin: Theme.horizontalPageMargin
                            verticalCenter: parent.verticalCenter
                        }
                        Label {
                            width: parent.width
                            text: candidate.modelData.name
                            textFormat: Text.PlainText
                            truncationMode: TruncationMode.Fade
                        }
                        Label {
                            width: parent.width
                            text: candidate.modelData.format
                            textFormat: Text.PlainText
                            font.pixelSize: Theme.fontSizeExtraSmall
                            color: candidate.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
                        }
                    }
                }
            }

            SectionHeader { text: qsTr("Export") }
            ValueButton {
                objectName: "exportButton"
                label: qsTr("Export a KeePass copy")
                description: qsTr("A KeePass file with its own password. It opens in KeePassXC, KeePassDX and Strongbox.")
                enabled: !ShoalKeys.busy
                onClicked: pageStack.push(Qt.resolvedUrl("ExportDialog.qml"))
            }

            SectionHeader { text: qsTr("Recovery key") }
            ValueButton {
                objectName: "recoveryKeyButton"
                label: qsTr("Save the recovery key file")
                description: qsTr("Your vault needs your master password and a key stored on this phone. Keep a copy of the key off the phone: with it and your password, you can open the vault after a reset or in KeePassXC. Anyone with both can read your passwords.")
                onClicked: {
                    var path = ShoalKeys.recoveryKeyPath()
                    var err = ShoalKeys.writeRecoveryKey(path)
                    page.messageIsError = err.length > 0
                    page.message = err.length ? err : qsTr("Saved to %1. Move it off the phone.").arg(path)
                    flick.scrollToTop()
                }
            }
        }
        VerticalScrollDecorator { }
    }
}
