// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Catalogue: every package tested on this phone's release, by category,
// with a search box and, above the list, the featured apps. Untested
// packages never reach this model (reef-backend hides them), the model is
// sorted by category, then title, so each section appears once, and typing
// in the search box narrows it (Reef.searchText).

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Reef 1.0
import "../components"
import "../components/Texts.js" as Texts

Page {
    id: page

    SilicaListView {
        id: list
        anchors.fill: parent
        model: Reef.catalogueModel

        PullDownMenu {
            busy: Reef.busy
            MenuItem {
                text: qsTr("Settings")
                onClicked: pageStack.push(Qt.resolvedUrl("SettingsPage.qml"))
            }
            MenuItem {
                text: qsTr("Licences")
                onClicked: pageStack.push(Qt.resolvedUrl("LicencesPage.qml"))
            }
            MenuItem {
                text: Reef.updateCount > 0
                      ? qsTr("Installed (%n update(s))", "", Reef.updateCount)
                      : qsTr("Installed")
                onClicked: pageStack.push(Qt.resolvedUrl("InstalledPage.qml"))
            }
            MenuItem {
                text: qsTr("Refresh")
                enabled: !Reef.busy
                onClicked: Reef.refresh()
            }
        }

        header: Column {
            width: list.width

            PageHeader {
                title: qsTr("Reef")
            }

            SearchField {
                id: search
                objectName: "search"
                width: parent.width
                placeholderText: qsTr("Search apps")
                text: Reef.searchText
                onTextChanged: Reef.searchText = text
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
            }

            // The featured apps, an editorial list in the catalogue; hidden
            // while searching, when the list below is the answer.
            Column {
                width: parent.width
                visible: Reef.featuredModel.count > 0 && Reef.searchText.length === 0

                SectionHeader { text: qsTr("Featured") }

                ListView {
                    id: featured
                    objectName: "featured"
                    width: parent.width
                    height: Theme.iconSizeLauncher + Theme.paddingSmall + Theme.fontSizeExtraSmall * 1.4 + 2 * Theme.paddingMedium
                    orientation: ListView.Horizontal
                    clip: true
                    model: Reef.featuredModel
                    header: Item { width: Theme.horizontalPageMargin - Theme.paddingLarge; height: 1 }
                    delegate: FeaturedDelegate {
                        onClicked: pageStack.push(Qt.resolvedUrl("PackagePage.qml"), { packageName: name })
                    }
                }
            }

            // The repository no longer matches the installed release (an OS
            // update happened) or was disabled: offer the fix up front.
            Column {
                width: parent.width
                visible: Reef.repoState !== "pinned"
                spacing: Theme.paddingSmall

                Label {
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    wrapMode: Text.Wrap
                    color: Theme.highlightColor
                    text: Reef.repoState === "drifted"
                          ? qsTr("Reef is set up for another Sailfish OS version. Repair it to get apps tested on %1.").arg(Reef.installedRelease)
                          : Reef.repoState === "disabled"
                            ? qsTr("Reef's app source is turned off.")
                            : qsTr("Reef's app source is missing.")
                }
                Button {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: qsTr("Repair repository")
                    enabled: !Reef.busy
                    onClicked: Reef.repairRepository()
                }
                Item { width: 1; height: Theme.paddingMedium }
            }
        }

        section.property: "category"
        section.delegate: SectionHeader {
            required property string section
            text: section
        }

        delegate: PackageDelegate {
            onClicked: pageStack.push(Qt.resolvedUrl("PackagePage.qml"), { packageName: name })
        }

        ViewPlaceholder {
            enabled: list.count === 0 && !Reef.busy
            text: Reef.searchText.length > 0 ? qsTr("No apps match")
                : Reef.lastError.length > 0 ? qsTr("Couldn't load the apps")
                                            : qsTr("No apps yet")
            hintText: Reef.searchText.length > 0 ? qsTr("Try another word, or clear the search.")
                      : Reef.lastError.length > 0
                      ? Texts.message(Reef.lastError)
                      : qsTr("No apps are tested on Sailfish OS %1 yet. Pull down to refresh.").arg(Reef.installedRelease)
        }

        VerticalScrollDecorator {}
    }

    BusyIndicator {
        anchors.centerIn: parent
        running: Reef.busy && list.count === 0
    }
}
