// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// One package: what it is, which Sailfish releases it was tested on, what
// it may access, and install / update / remove. Everything shown here comes
// from the developer's catalogue entry: it is plain text (the window's
// default label format), and only https:// links are ever opened.

pragma ComponentBehavior: Bound

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Reef 1.0
import "../components"
import "../components/Texts.js" as Texts

Page {
    id: page

    property string packageName
    // Re-read whenever the models are rebuilt (a refresh, an operation, a
    // licence change): reading Reef.revision makes the binding depend on it.
    readonly property var pkg: {
        const revision = Reef.revision
        return revision >= 0 ? Reef.packageDetails(packageName) : ({})
    }
    readonly property bool working: Reef.busy && Reef.busyPackage === packageName
    readonly property bool paid: pkg.licenceModel === "one_off" || pkg.licenceModel === "subscription"
    // Why the last install, update or remove of this package failed.
    property string error

    Connections {
        target: Reef
        function onOperationFinished(name, ok, message) {
            if (name === page.packageName)
                page.error = ok ? "" : Texts.message(message)
        }
    }

    RemorsePopup { id: remorse }

    readonly property bool installed: pkg.installState !== undefined && pkg.installState !== "not_installed"
    readonly property bool canBuy: paid && !pkg.licensed && Texts.isHttps(pkg.purchaseUrl)
    readonly property bool showActions: !working && (!installed || pkg.installState === "update_available" || canBuy)

    // Set once Buy has opened the checkout in the browser.
    property bool buying

    // Opens the checkout with a new claim code in its URL; Reef fetches
    // the licence with that code once the purchase is paid.
    function buy() {
        const url = Reef.buyUrl(page.packageName)
        if (url.length === 0) {
            page.error = Texts.message(Reef.lastError)
            return
        }
        page.error = ""
        page.buying = true
        Texts.openHttps(url)
    }

    function install() {
        page.error = ""
        if (page.pkg.installState === "update_available")
            Reef.update(page.packageName)
        else
            Reef.install(page.packageName)
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        // Removing is a pull-down action with a remorse timer, as in the
        // Jolla Store; the links to the developer's pages sit with it.
        PullDownMenu {
            busy: page.working
            visible: page.installed || Texts.isHttps(page.pkg.source) || Texts.isHttps(page.pkg.homepage)
            MenuItem {
                objectName: "sourceItem"
                text: qsTr("Source code")
                visible: Texts.isHttps(page.pkg.source)
                onClicked: Texts.openHttps(page.pkg.source)
            }
            MenuItem {
                objectName: "homepageItem"
                text: qsTr("Homepage")
                visible: Texts.isHttps(page.pkg.homepage)
                onClicked: Texts.openHttps(page.pkg.homepage)
            }
            MenuItem {
                objectName: "removeItem"
                text: qsTr("Remove")
                visible: page.installed
                enabled: !Reef.busy
                onClicked: remorse.execute(qsTr("Removing %1").arg(page.pkg.title),
                                           function() { page.error = ""; Reef.remove(page.packageName) })
            }
        }

        Column {
            id: column
            width: page.width

            PageHeader {
                title: page.pkg.title || ""
                description: page.pkg.summary || ""
            }

            // The icon, and who publishes the app.
            Item {
                width: parent.width
                height: appIcon.height + Theme.paddingMedium

                AppIcon {
                    id: appIcon
                    x: Theme.horizontalPageMargin
                    iconPath: page.pkg.iconPath || ""
                    title: page.pkg.title || ""
                    size: Theme.iconSizeExtraLarge
                }

                Column {
                    anchors {
                        left: appIcon.right
                        leftMargin: Theme.paddingLarge
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: appIcon.verticalCenter
                    }

                    Label {
                        objectName: "publisher"
                        width: parent.width
                        visible: !!page.pkg.publisher
                        text: page.pkg.publisher || ""
                        textFormat: Text.PlainText
                        color: Theme.highlightColor
                        truncationMode: TruncationMode.Fade
                    }
                    Label {
                        width: parent.width
                        text: page.pkg.category || ""
                        textFormat: Text.PlainText
                        color: Theme.secondaryHighlightColor
                        font.pixelSize: Theme.fontSizeSmall
                        truncationMode: TruncationMode.Fade
                    }
                }
            }

            // The one thing to do with this app: install, update, or buy.
            // Paid packages install freely (the app checks its own licence
            // offline); buying happens on the web, through the merchant of
            // record.
            // (A Row, not ButtonLayout: Keel's ButtonLayout does not lay
            // out its buttons inside a Column yet.)
            Item {
                width: parent.width
                height: page.showActions ? actions.height + 2 * Theme.paddingMedium : 0
                visible: page.showActions
                Row {
                    id: actions
                    objectName: "actions"
                    anchors.centerIn: parent
                    spacing: Theme.paddingLarge
                    Button {
                        objectName: "installButton"
                        visible: page.pkg.installState === "not_installed" || page.pkg.installState === "update_available"
                        enabled: !Reef.busy
                        text: page.pkg.installState === "update_available" ? qsTr("Update to %1").arg(page.pkg.updateVersion || page.pkg.version)
                            : page.error.length > 0 ? qsTr("Try again") : qsTr("Install")
                        onClicked: page.install()
                    }
                    // One action at a time: install first (paid apps install
                    // freely and check their licence offline), then buy.
                    Button {
                        objectName: "buyButton"
                        visible: page.canBuy && page.pkg.installState === "installed"
                        text: page.pkg.licenceModel === "subscription" ? qsTr("Subscribe") : qsTr("Buy")
                        onClicked: page.buy()
                    }
                }
            }

            // After Buy: the licence arrives by itself once paid.
            Label {
                objectName: "buyHint"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: page.buying && page.canBuy
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                color: Theme.secondaryHighlightColor
                font.pixelSize: Theme.fontSizeSmall
                text: qsTr("Finish the purchase in the browser. Reef adds the licence when you come back.")
            }

            // Installed and current: say so, instead of a button.
            Label {
                objectName: "installedNote"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: !page.working && page.pkg.installState === "installed"
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                text: qsTr("Installed and up to date")
            }

            ProgressBar {
                objectName: "progress"
                width: parent.width
                visible: page.working
                minimumValue: 0
                maximumValue: 100
                indeterminate: Reef.progress < 0
                value: Math.max(0, Reef.progress)
                valueText: Reef.progress >= 0 ? qsTr("%1%").arg(Reef.progress) : ""
                label: Texts.busyText(Reef.busyText)
            }

            Label {
                objectName: "operationError"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                topPadding: Theme.paddingMedium
                visible: page.error.length > 0 && !page.working
                wrapMode: Text.Wrap
                color: Theme.errorColor
                font.pixelSize: Theme.fontSizeSmall
                text: qsTr("That didn't work: %1").arg(page.error)
            }

            // Screenshots the repository ships (cached files, like the icon);
            // a horizontal strip, each at the phone's own proportions. A tap
            // opens them full screen (ScreenshotPage).
            SectionHeader { text: qsTr("Screenshots"); visible: shots.count > 0 }
            Flickable {
                id: shotStrip
                objectName: "screenshots"
                width: parent.width
                height: visible ? Math.round(page.height * 0.42) : 0
                visible: shots.count > 0
                contentWidth: shotRow.width
                flickableDirection: Flickable.HorizontalFlick
                clip: true

                Row {
                    id: shotRow
                    height: shotStrip.height
                    spacing: Theme.paddingMedium

                    Item { width: Theme.horizontalPageMargin - shotRow.spacing; height: 1 }
                    Repeater {
                        id: shots
                        model: page.pkg.screenshotPaths || []
                        // Tapped: the screenshots full screen, from this one.
                        BackgroundItem {
                            id: shot
                            required property string modelData
                            required property int index
                            objectName: "screenshot" + index
                            height: parent.height
                            width: Math.round(height * 9 / 16)
                            onClicked: pageStack.push(Qt.resolvedUrl("ScreenshotPage.qml"), {
                                paths: page.pkg.screenshotPaths,
                                startIndex: shot.index
                            })

                            Image {
                                anchors.fill: parent
                                source: "file://" + shot.modelData
                                sourceSize.height: height
                                fillMode: Image.PreserveAspectFit
                                asynchronous: true
                                smooth: true
                            }
                        }
                    }
                    Item { width: Theme.horizontalPageMargin - shotRow.spacing; height: 1 }
                }

                HorizontalScrollDecorator {}
            }

            SectionHeader { text: qsTr("Description"); visible: !!page.pkg.description }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: !!page.pkg.description
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.highlightColor
                text: page.pkg.description || ""
            }

            SectionHeader { text: qsTr("Details") }
            DetailItem { label: qsTr("Version"); value: page.pkg.version || "" }
            DetailItem {
                label: qsTr("Installed")
                value: page.pkg.installedVersion || ""
                visible: page.pkg.installState !== undefined && page.pkg.installState !== "not_installed"
            }
            DetailItem { label: qsTr("Category"); value: page.pkg.category || "" }
            DetailItem { label: qsTr("Published"); value: page.pkg.added || ""; visible: !!page.pkg.added }
            DetailItem {
                label: qsTr("Price")
                value: page.pkg.licenceModel === "one_off" ? qsTr("One-off licence")
                     : page.pkg.licenceModel === "subscription" ? qsTr("Subscription")
                     : qsTr("Free")
            }
            DetailItem { label: qsTr("Licence"); value: page.pkg.spdx || ""; visible: !!page.pkg.spdx }
            DetailItem {
                // The store's Keel grade from the catalogue's keel_tier, the
                // engineering tier keel-compat measures (docs/plan.md,
                // "Compatibility target", where B is the higher): grade A
                // is tier B reached, grade B is tier A only.
                label: qsTr("Keel grade")
                value: page.pkg.keelTier === "B" ? qsTr("A: everything it uses runs on Keel")
                     : page.pkg.keelTier === "A" ? qsTr("B: some components differ")
                     : page.pkg.keelTier === "none" ? qsTr("Not supported yet")
                     : page.pkg.keelTier === "not-applicable" ? qsTr("Not applicable (no Silica)")
                     : (page.pkg.keelTier || "")
                visible: !!page.pkg.keelTier
            }
            DetailItem {
                label: qsTr("Download size")
                value: page.pkg.sizeText || ""
                visible: !!page.pkg.sizeText
            }

            SectionHeader { text: qsTr("Tested on") }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.secondaryHighlightColor
                font.pixelSize: Theme.fontSizeSmall
                text: page.pkg.deviceTested === false
                      ? qsTr("Built for this phone's Sailfish OS (%1), but not yet tested on a phone running it. "
                             + "If something doesn't work, please report it.").arg(Reef.installedRelease)
                      : qsTr("Reef only offers versions tested on this phone's Sailfish OS (%1).").arg(Reef.installedRelease)
            }
            Repeater {
                model: page.pkg.deviceTested === false ? [] : (page.pkg.testedOn || [])
                DetailItem {
                    required property string modelData
                    label: qsTr("Sailfish OS")
                    value: modelData === Reef.installedRelease ? qsTr("%1 (this phone)").arg(modelData)
                                                               : modelData
                }
            }

            SectionHeader { text: qsTr("Permissions") }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: page.pkg.sandboxed === false
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeSmall
                text: qsTr("Runs outside the Sailfish OS sandbox, with full access to your files and data.")
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: page.pkg.sandboxed === true && (!page.pkg.permissions || page.pkg.permissions.length === 0)
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.highlightColor
                text: qsTr("Runs in the Sailfish OS sandbox, with no extra permissions.")
            }
            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                visible: !!page.pkg.sandboxed && !!page.pkg.permissions && page.pkg.permissions.length > 0
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.highlightColor
                text: visible ? qsTr("Runs in the Sailfish OS sandbox and may use: %1.")
                                .arg(page.pkg.permissions.join(", ")) : ""
            }

        }

        VerticalScrollDecorator {}
    }
}
