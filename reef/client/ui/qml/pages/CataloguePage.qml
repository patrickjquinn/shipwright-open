// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// The store's home: updates waiting (when there are), the featured apps as
// a banner, the newest and most recently updated apps, then every app from
// A to Z. Search is in the pull-down menu, as in the Jolla Store; while it
// is open, the list below is the answer and the rest steps aside.
// Untested packages never reach these models (reef-backend hides them).

pragma ComponentBehavior: Bound

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Reef 1.0
import "../components"
import "../components/Texts.js" as Texts

Page {
    id: page

    // The search field shows while searching: opened from the pull-down
    // menu, and kept while it holds text.
    property bool searching: Reef.searchText.length > 0
    // The search field in the list's header, for the pulley's Search.
    property Item searchField
    readonly property bool browsing: !searching

    function openApp(name) {
        pageStack.push(Qt.resolvedUrl("PackagePage.qml"), { packageName: name })
    }

    // A flickable with one column (as Silica's own mixed pages): the page
    // header, then the banners and rows, then every app. When something near
    // the top appears or grows (the updates banner arrives after the page),
    // what is below moves down and the view stays where the person left it.
    // A ListView header grows upwards instead (Qt keeps the first row in
    // place), which slid the title and the banner under the camera cut-out.
    SilicaFlickable {
        id: list
        anchors.fill: parent
        contentHeight: content.height
        readonly property int count: rows.count

        PullDownMenu {
            busy: Reef.busy
            MenuItem {
                text: qsTr("Settings")
                onClicked: pageStack.push(Qt.resolvedUrl("SettingsPage.qml"))
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
            MenuItem {
                objectName: "searchItem"
                text: qsTr("Search")
                onDelayedClick: {
                    page.searching = true
                    list.scrollToTop()
                    if (page.searchField)
                        page.searchField.forceActiveFocus()
                }
            }
        }

        Column {
            id: content
            width: list.width
            Component.onCompleted: page.searchField = search

            PageHeader {
                title: qsTr("Reef")
            }

            SearchField {
                id: search
                objectName: "search"
                width: parent.width
                visible: page.searching
                // Sized by its own condition, not `visible`: `visible` is also false while the page is hidden under another, and a height that followed it collapsed the page's content then (its scroll position was lost on the way back).
                height: (page.searching) ? implicitHeight : 0
                placeholderText: qsTr("Search apps")
                text: Reef.searchText
                onTextChanged: Reef.searchText = text
                EnterKey.iconSource: "image://theme/icon-m-enter-close"
                EnterKey.onClicked: focus = false
                // Emptied and left: back to the home page.
                onActiveFocusChanged: if (!activeFocus && text.length === 0) page.searching = false
            }

            // The repository no longer matches the installed release (an OS
            // update happened) or was disabled: offer the fix up front.
            Column {
                width: parent.width
                visible: Reef.repoState !== "pinned"
                spacing: Theme.paddingSmall
                bottomPadding: Theme.paddingLarge

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
            }

            // Updates waiting: first, with the one button that does them all.
            BackgroundItem {
                objectName: "updatesCard"
                width: parent.width
                // Room for a title on up to three lines ("3 updates for
                // libraries and services" beside the button was cut off).
                height: (page.browsing && Reef.updateCount > 0) ? Math.max(Theme.itemSizeLarge, updatesText.height + 2 * Theme.paddingMedium) : 0
                visible: page.browsing && Reef.updateCount > 0
                onClicked: pageStack.push(Qt.resolvedUrl("InstalledPage.qml"))

                Rectangle {
                    anchors.fill: parent
                    color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity * 0.6)
                }
                Icon {
                    id: updatesIcon
                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    source: "image://theme/icon-m-cloud-download"
                    color: Theme.highlightColor
                }
                Column {
                    id: updatesText
                    anchors.left: updatesIcon.right
                    anchors.leftMargin: Theme.paddingLarge
                    anchors.right: updateAll.left
                    anchors.rightMargin: Theme.paddingMedium
                    anchors.verticalCenter: parent.verticalCenter
                    Label {
                        width: parent.width
                        // Apps, and the libraries and services they use:
                        // Update all lists them all before applying them.
                        text: Texts.updatesText(Reef.appUpdateCount, Reef.updateCount)
                        color: Theme.highlightColor
                        wrapMode: Text.WordWrap
                        maximumLineCount: 3
                        truncationMode: TruncationMode.Fade
                    }
                    Label {
                        width: parent.width
                        text: qsTr("Tap to see what's new")
                        color: Theme.secondaryHighlightColor
                        font.pixelSize: Theme.fontSizeExtraSmall
                        truncationMode: TruncationMode.Fade
                    }
                }
                SecondaryButton {
                    id: updateAll
                    objectName: "updateAllButton"
                    anchors.right: parent.right
                    anchors.rightMargin: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Update all")
                    enabled: !Reef.busy
                    onClicked: Reef.updateAll()
                }
            }

            // The featured apps, one banner at a time, turning every few
            // seconds while the page is in front.
            Item {
                width: parent.width
                height: (page.browsing && Reef.featuredModel.count > 0) ? hero.height + dots.height + Theme.paddingMedium : 0
                visible: page.browsing && Reef.featuredModel.count > 0

                SlideshowView {
                    id: hero
                    objectName: "featured"
                    // Its neighbouring banners wait a page width to either
                    // side; unclipped, they slid across the next page while
                    // the page stack moved this one away (a ghost of the
                    // carousel over Installed).
                    clip: true
                    width: parent.width
                    height: Math.round(Math.min(page.width, page.height) * 0.72)
                    itemWidth: width
                    itemHeight: height
                    model: Reef.featuredModel
                    delegate: HeroCard {
                        width: hero.itemWidth
                        height: hero.itemHeight
                        onClicked: page.openApp(name)
                    }
                }
                Timer {
                    interval: 6000
                    repeat: true
                    running: hero.count > 1 && page.status === PageStatus.Active
                             && Qt.application.state === Qt.ApplicationActive && !hero.moving
                    onTriggered: hero.incrementCurrentIndex()
                }
                Row {
                    id: dots
                    anchors.top: hero.bottom
                    anchors.topMargin: Theme.paddingMedium
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: Theme.paddingSmall
                    visible: hero.count > 1
                    height: (hero.count > 1) ? Theme.paddingSmall : 0
                    Repeater {
                        model: hero.count
                        Rectangle {
                            required property int index
                            width: Theme.paddingSmall
                            height: width
                            radius: width / 2
                            color: index === hero.currentIndex ? Theme.highlightColor : Theme.secondaryColor
                            opacity: index === hero.currentIndex ? 1.0 : Theme.opacityLow
                        }
                    }
                }
            }

            // New apps and new versions, newest first.
            SectionHeader {
                text: qsTr("New and updated")
                visible: page.browsing && Reef.recentModel.count > 0
            }
            SilicaListView {
                objectName: "recent"
                width: parent.width
                height: (page.browsing && Reef.recentModel.count > 0) ? Theme.iconSizeLauncher + 2 * Theme.fontSizeExtraSmall * 1.4 + 2 * Theme.paddingMedium : 0
                visible: page.browsing && Reef.recentModel.count > 0
                orientation: ListView.Horizontal
                clip: true
                model: Reef.recentModel
                header: Item { width: Theme.horizontalPageMargin - Theme.paddingLarge; height: 1 }
                delegate: FeaturedDelegate {
                    onClicked: page.openApp(name)
                }
            }

            SectionHeader {
                text: page.searching ? qsTr("Results") : qsTr("All apps")
                visible: list.count > 0
            }

            Repeater {
                id: rows
                model: Reef.catalogueModel
                delegate: PackageDelegate {
                    onClicked: page.openApp(name)
                }
            }
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
        size: BusyIndicatorSize.Large
        running: Reef.busy && list.count === 0
    }
}
