<!-- SPDX-FileCopyrightText: 2026 Patrick Quinn -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Changes from upstream

Upstream: xmatic by JimKnopfIoT, https://github.com/JimKnopfIoT/harbour-xmatic,
tag `v0.41.0`, commit `40ce16b482d5a8afe87e0dfc5e6910994a5c2f1a` (see
`UPSTREAM.md`). Everything below was done by Shipwright in 2026. Every
upstream file whose content we changed carries the line "Modified by
Shipwright, 2026: rebranded as Shoal Messages; see CHANGES-FROM-UPSTREAM.md"
(Apache-2.0 section 4(b)), except `core/Cargo.lock`, which Cargo regenerates
and would strip a comment from; its only change is the package rename and
version.

Diff against upstream: export upstream at the commit above, apply the file
renames in section 1, then `diff -ru`.

## 2026-09-30: import and rebrand (fork version 0.1.0)

### 1. Files renamed

| Upstream | Shoal Messages |
|---|---|
| `harbour-xmatic.pro` | `shipwright-shoal-messages.pro` |
| `harbour-xmatic.desktop` | `shipwright-shoal-messages.desktop` |
| `harbour-xmatic-open-url.desktop` | `shipwright-shoal-messages-open-url.desktop` |
| `org.xmatic.xmatic.service` | `org.shipwright.ShoalMessages.service` |
| `org.unifiedpush.Connector.xmatic.service` | `org.unifiedpush.Connector.ShoalMessages.service` |
| `XmaticSpeech.permission` | `ShoalMessagesSpeech.permission` |
| `rpm/harbour-xmatic.spec` | `rpm/shipwright-shoal-messages.spec` |
| `qml/harbour-xmatic.qml` | `qml/shipwright-shoal-messages.qml` |
| `src/harbour-xmatic.cpp` | `src/shipwright-shoal-messages.cpp` |
| `translations/harbour-xmatic-<lang>.ts` (32 files) | `translations/shipwright-shoal-messages-<lang>.ts` |

### 2. Files removed

- `icons/{86x86,108x108,128x128,172x172}/harbour-xmatic.png`,
  `icons/xmatic-logo.svg`, `qml/images/xmatic-mark.png`: upstream branding.
- `screenshots/` (15 JPEGs of the upstream app, used only by upstream's README).

### 3. Files added (Shipwright's own, Apache-2.0)

- `NOTICE`, `UPSTREAM.md`, `CHANGES-FROM-UPSTREAM.md`, `ROADMAP.md`.
- `src/shoalconfig.h`: the single config location for the default push gateway.
- `icons/shoal-messages-logo.svg` and the four launcher PNGs rendered from it
  (`icons/<size>/shipwright-shoal-messages.png`); `qml/images/shoal-messages-mark.svg`
  and its 512 px render `shoal-messages-mark.png` for the cover. Placeholder
  artwork, drawn from scratch.

### 4. Identifier mapping (applied across the whole tree)

| Upstream | Shoal Messages | Where |
|---|---|---|
| `harbour-xmatic` | `shipwright-shoal-messages` | package, binary, `TARGET`, desktop ids, icon name, `/usr/share/<name>`, translation catalogues, QML entry file |
| `OrganizationName=org.xmatic`, `ApplicationName=xmatic` | `org.shipwright`, `ShoalMessages` | both desktop files; Sailjail derives the app's D-Bus name and data/config directories from this pair |
| `org.xmatic.xmatic`, `/org/xmatic/xmatic` | `org.shipwright.ShoalMessages`, `/org/shipwright/ShoalMessages` | app D-Bus service, interface and object path (`src/appservice.*`, `src/instancelock.cpp`, `qml/shipwright-shoal-messages.qml` notification remote actions), Matrix pusher `app_id` (`core/src/push.rs`) |
| `org.unifiedpush.Connector.xmatic` | `org.unifiedpush.Connector.ShoalMessages` | UnifiedPush connector bus name (`core/src/push.rs`, `src/pushwake.cpp`, activation file) |
| `XmaticSpeech` | `ShoalMessagesSpeech` | Sailjail permission for the speech service |
| `xmatic-core` / `xmatic_core` / `libxmatic_core.a` / `xmatic_core.h` / `XMATIC_CORE_H` | `shoal-messages-core` / `shoal_messages_core` / `libshoal_messages_core.a` / `shoal_messages_core.h` / `SHOAL_MESSAGES_CORE_H` | Rust crate, lib, cbindgen header (`core/Cargo.toml`, `core/Cargo.lock`, `core/cbindgen.toml`, `scripts/build-core.sh`, `.pro`, `src/matrixbridge.h`) |
| `XMATIC_*` macros and environment variables (`XMATIC_PUSH_WAKE`, `XMATIC_VOICE_*`, `XMATIC_VERSION`) and `xmatic_version.h` | `SHOAL_MESSAGES_*`, `shoal_messages_version.h` | `src/`, `.pro`, spec, activation file |
| `image://xmatic-emoji/`, internal link scheme `xmatic:`, temp/file prefixes `xmatic-*`, thread name, tracing targets, log prefix `xmatic:` | `shoal-messages` equivalents | `src/`, `core/src/`, `qml/` |
| Sailfish Secrets collection `xmatic` | `ShoalMessages` | `src/secretskeeper.cpp` |
| Store marker text `xmatic store key v1` | `shoal-messages store key v1` | `core/src/session.rs` (only existence is checked) |

The FFI function prefix `xm_` is internal and unchanged.

Consequence: a device that ran xmatic keeps its xmatic data; Shoal Messages
starts with an empty store under its own directory and its own Secrets
collection. There is no migration from xmatic, by design (different product).

### 5. Visible strings

- Every user-visible "xmatic" in `qsTr()`/`tr()` strings, the desktop `Name=`,
  cover title, About page, notification app name (`appName` in QML,
  `Notify` app name in `src/pushwake.cpp`), PulseAudio client and stream names
  (`src/callaudiorouter.cpp`, `src/callengine.cpp`), OAuth client name and
  initial device name (`core/src/login.rs`), and pusher display names
  (`core/src/push.rs`) now read "Shoal Messages". Comments were reworded the
  same way.
- Translations: the same replacement in the `<source>` and `<translation>`
  of all 32 `.ts` files, so sources still match the code. Genitive `xmatic's`
  / `xmatics` became `Shoal Messages'`. Inflected forms: suffix kept in
  Finnish, Estonian and Hungarian (for example `Shoal Messagesin`), dropped in
  Czech, Slovak, Croatian, Polish and Slovenian. Needs native review.
- About page (`qml/pages/AboutPage.qml`): title and name "Shoal Messages";
  the "Source code" button is replaced by the credit line "Based on xmatic by
  JimKnopfIoT, Apache-2.0" and an "Upstream project" button linking to the
  upstream repository.
- Placeholder URLs, to be replaced before release:
  `core/src/login.rs` `CLIENT_URI` and the tile-server User-Agent in
  `core/src/location.rs` now name `https://shipwright.example/shoal/messages`.

### 6. Push integration

- `core/src/push.rs`: new `SHOAL_DISTRIBUTOR = "org.unifiedpush.Distributor.shoal"`
  and `choose_distributor()` (user's pick, then `UNIFIEDPUSH_DISTRIBUTOR`, then
  Shoal Push, then first sorted name), replacing upstream's
  `try_use_default_distributor()`-then-first logic. `Command::Enable` carries
  the user's pick. `push.state` additionally reports `automatic` (what
  Automatic would choose now) and `shoalDistributor`. Six new unit tests.
- `core/src/protocol.rs`: `push.enable` takes an optional `distributor`.
- `core/src/runtime.rs`: passes it through to the connector.
- `src/matrixbridge.{h,cpp}`: `enablePush(gateway, distributor = "")`.
- `src/appsettings.{h,cpp}`: `pushGateway` falls back to
  `SHOAL_DEFAULT_PUSH_GATEWAY` (`src/shoalconfig.h`,
  `https://push.shipwright.example/_matrix/push/v1/notify`, placeholder);
  storing the default or an empty value stores "unset". New
  `defaultPushGateway` (constant) and `pushDistributor` (`push/distributor`)
  properties.
- `qml/pages/PushPage.qml`: shows the distributor in use (named "Shoal Push"
  for ours); new "Use distributor" combo box (Automatic plus every distributor
  on the bus), disabled while push is on; gateway text and placeholder
  describe the default and how to return to it.
- `docs/PUSH.md`: "What you need" and "Status" updated for Shoal Push, the
  default gateway and distributor choice; a note on the ntfy chain.
- Unchanged on purpose: push remains opt-in, the pusher stays `event_id_only`,
  and sign-out still removes pushers and the registration.

### 7. Packaging and build

- `rpm/shipwright-shoal-messages.spec`: `Name`, `Summary`, `Version: 0.1.0`,
  `URL` (placeholder), description with the
  fork credit, `%license NOTICE`, renamed file lists, and a new top changelog
  entry. Upstream's changelog entries are kept below it as history.
- `core/Cargo.toml` and `core/Cargo.lock`: crate `shoal-messages-core`
  version `0.1.0` (must equal the spec's `Version`; `scripts/build-core.sh`
  and the `.pro` enforce it). Dependencies and the toolchain pin are untouched.
- `shipwright-shoal-messages.pro`: renamed sources, files and macros; adds
  `src/shoalconfig.h`.
- `scripts/build-core.sh`, `scripts/desktop-check.py`,
  `tools/translation-review.py`, `translations/STATUS.md`: renamed paths.
- `.gitignore`: ignore rules anchored to this directory; upstream's `docs/*`,
  `tools/` and unanchored `STATUS.md` exclusions removed because inside the
  monorepo they hid tracked files.
- `THIRD-PARTY.md`: first paragraph names the fork. The crate list is
  unchanged (no dependency changed).
- `README.md`: rewritten for Shoal Messages.

## 2026-09-30: hosted push account from the subscription bundle

Shoal Push (`shoal/push/`) exposes `org.shipwright.ShoalPush1.Configure(a{sv})`
on `org.shipwright.ShoalPush`, `/org/shipwright/ShoalPush`, reachable only with
the `ShoalPush` Sailjail permission. The onboarding bundle from
`services/deploy/provision-subscriber.sh` has a `push` section
`{server, gateway, token, topic_prefix}`.

- `shipwright-shoal-messages.desktop`, `shipwright-shoal-messages-open-url.desktop`:
  `ShoalPush` added to `Permissions` (both files, as `scripts/desktop-check.py`
  requires), with a comment on why.
- `core/src/hosted.rs` (new): `parse_bundle` (whole bundle or just its `push`
  section; https `server` and `gateway` required; `token` may be null, which
  is left out of the call so the distributor keeps its token; `topic_prefix`
  sanity-checked), `HostedPush` with a `Debug` that redacts the token,
  `redact` (token and ids out of any error text), `configure_on` (the D-Bus
  call; ServiceUnknown/NameHasNoOwner become "Shoal Push is not installed or
  not running", AccessDenied names the missing permission, InvalidArgs is
  passed on redacted) and `configure` (session bus, on its own thread under
  `zbus::block_on`, as `push.rs` does). Five tests: bundle parsing, refusal
  messages, token redaction, and a round trip against a private
  `dbus-daemon` with a fake `org.shipwright.ShoalPush` (absent service,
  success with `moved`, rejection whose text echoed the token).
- `core/src/protocol.rs`: new `push.configureHosted { bundle: Secret, endpoint,
  p256dh, auth }` (`Secret`, because a full bundle also carries the Matrix
  initial password).
- `core/src/runtime.rs`: `push_configure_hosted`: parse, `Configure`, then, if
  an endpoint is known, `set_pusher` with the bundle's gateway; replies
  `{gateway, moved, pusher: "registered"|"pending"}`.
- `core/src/lib.rs`: `mod hosted`.
- `src/matrixbridge.{h,cpp}`: `applySubscriptionBundle(bundle)` sends the
  command with the current endpoint, payload wiped after sending, nothing
  logged but the fact; on success stores the gateway (`m_pushGateway` and
  `AppSettings::pushGateway`) and emits `hostedPushApplied(moved,
  pusherRegistered)`; on failure `hostedPushFailed(reason)` instead of a
  banner.
- `qml/pages/SubscriptionPage.qml` (new): paste, paste from clipboard (then
  cleared) or open a `.json` file; optional "Switch on push notifications";
  the field is emptied on apply. `qml/pages/AccountPage.qml`: "Shipwright
  subscription" button. `shipwright-shoal-messages.pro`: page listed.
- `core/Cargo.lock` unchanged (zbus 5 was already a dependency).

## 2026-10-01: Phase 2 (cover action, bridges onboarding, contact matching)

Every changed upstream file carries a "Modified by Shipwright, 2026" line
naming the change (files changed earlier keep their line, extended).

### Cover: mark all read

- `core/src/readall.rs` (new): `select` (joined, non-space rooms with unread
  messages, unread notifications or the unread flag), `mark_each` (bounded
  concurrency, 4) and `mark_all` over `roomlist::mark_read`. Three tests.
- `core/src/protocol.rs`: `rooms.markAllRead {receipt}`; `core/src/runtime.rs`:
  `mark_all_read`, answering `{marked, skipped, failed}`.
- `src/matrixbridge.{h,cpp}`: `Q_INVOKABLE markAllRead()` (receipt from
  `AppSettings::sendReadReceipts`), signal `markedAllRead(marked, failed)`.
- `qml/cover/CoverPage.qml`: `CoverActionList` with one `CoverAction`,
  enabled when signed in and `unreadRooms > 0`.

### Hosted bridges onboarding

- `core/src/bridges.rs` (new): the bridgev2 bot command contract (texts
  from mautrix-go `bridgev2/commands`, mautrix-signal and mautrix-telegram
  connectors), reply parsing, per-bridge state (`BridgeState`), the
  `Registry` shared with a sync event handler, the replay guard (marker
  field `org.shipwright.bridge_request`), QR encoding to module rows, and
  the Matrix glue (`transmit`, `send`, `wait_for_join`, `install`). Tests
  against a scripted bridgev2 bot (11 tests).
- `core/src/protocol.rs`: `bridges.status`, `bridges.login`,
  `bridges.submit` (value as `Secret`), `bridges.cancel`, `bridges.logout`.
- `core/src/runtime.rs`: `bridge_room` (the bot's direct chat via
  `timeline::direct_chat`, subscribed, waiting for the bot to join),
  `bridges_status` (waits for the login list, 20 s), `bridges_request`;
  `Subscriptions.bridges`; `bridges::install` next to the verification and
  call handlers (four places); the registry is cleared in `stop_observers`;
  `push.configureHosted` also answers the bundle's bridge bots.
- `core/Cargo.toml`, `core/Cargo.lock`: `qrcode = "=0.14.1"`, default
  features off (no dependencies of its own). `THIRD-PARTY.md`: listed, count
  570.
- `src/bridgeactions.{h,cpp}` (new): `matrix.bridges` for QML; per-bridge
  state, commands, replies by id, bots from the bundle.
- `src/matrixbridge.{h,cpp}`: owns `BridgeActions`, routes `bridges.*`
  replies and `bridge.state` events to it, stores the bundle's bots,
  resets on sign-out.
- `src/appsettings.{h,cpp}`: `bridgeBot(bridge)` / `setBridgeBot`
  (`bridges/<bridge>Bot`).
- `qml/pages/BridgesPage.qml`, `qml/pages/BridgeLinkPage.qml` (new);
  `qml/pages/AccountPage.qml` ("Signal and Telegram"),
  `qml/pages/SubscriptionPage.qml` (next step after the bundle).
  `shipwright-shoal-messages.pro`: the new sources and pages.

### Native contacts (opt-in)

- `core/src/contacts.rs` (new): phone normalisation, Matrix-ID parsing,
  the in-memory `Index` and its `lookup` (Matrix ID, bridge identifiers,
  numeric display names; unique matches only). Ten tests.
- `core/src/protocol.rs`: `contacts.setIndex {contacts, countryCode}`,
  `contacts.clear` (the list's `Debug` prints only its length).
- `core/src/members.rs`: `contact_of`; `contact` on member-list rows (when
  matching is on) and on `member.profile`.
- `src/contactsbridge.{h,cpp}` (new): QtContacts read of the address book
  (qtcontacts-sqlite), only while the setting is on, re-read on changes.
- `src/appsettings.{h,cpp}`: `contactsMatching` (`privacy/contactsMatching`,
  default off).
- `src/membermodel.{h,cpp}`: `contactName` role.
- `qml/pages/PrivacyPage.qml` (switch and status),
  `qml/pages/MemberListPage.qml`, `qml/pages/MemberProfilePage.qml`.
- `shipwright-shoal-messages.desktop`, `shipwright-shoal-messages-open-url.desktop`:
  `Contacts` permission, with a comment.
- `shipwright-shoal-messages.pro`: `QT += contacts`.
  `rpm/shipwright-shoal-messages.spec`: `BuildRequires: pkgconfig(Qt5Contacts)`,
  `Requires: qtcontacts-sqlite-qt5`.

### Documentation

- `README.md`: cover, bridges, contacts and Keel sections; device
  verification steps. `ROADMAP.md`: status per item. `docs/KEEL-PORT.md`
  (new): the Keel port assessment.

## 2026-10-01: review fixes and Phase 2 polish (second pass)

Review findings E-1 to E-4 and E-6 (`docs/quality/findings/shoal-apps.md`),
and the Phase 2 items left open in the first pass.

### Review fixes

- E-1, `core/src/push.rs`: `PusherStep` and `after_configure` (wait, register
  again, or drop the stale pusher when Shoal Push moved the registration);
  test. `core/src/runtime.rs` `push_configure_hosted` deletes the old
  endpoint's pusher when `moved > 0` instead of registering it with the new
  gateway. `src/matrixbridge.{h,cpp}`: remembers the endpoint the bundle was
  sent with, and registers a fresh endpoint that arrived meanwhile again with
  the bundle's gateway.
- E-2, `src/contactsbridge.cpp`: switching matching off cancels the fetch,
  disconnects it and the manager, and deletes the manager with `deleteLater`;
  both pointers are nulled.
- E-3, `src/contactsbridge.cpp`: logging category `shoal.messages.contacts`
  (`qCWarning`, `qCInfo`).
- E-4: the locale table in `src/contactsbridge.cpp` is gone. New
  `core/src/dialling.rs` (dialling plan: calling code, national and
  international prefix; by MCC or ISO region; four tests) over the new data
  file `core/data/mcc-dialling.tsv` (all 241 MCCs, generated by the new
  `tools/mcc-dialling.py` from AOSP `MccTable.java` and libphonenumber
  `PhoneNumberMetadata.xml` v9.0.40, both Apache-2.0; SPDX lines and sources in
  the file). `core/src/contacts.rs`: `normalize_phone` and `Index::new` take a
  `Plan`; `plan_json`, `name_for`, `international`; two more tests.
  `core/src/protocol.rs`: `contacts.setIndex` takes `mcc` and `region`
  instead of `countryCode` and answers the plan in use (`dialling`).
  `src/contactsbridge.{h,cpp}`: asks oFono asynchronously for the SIM's
  mobile country code (SimManager, else NetworkRegistration, every modem),
  sends it with the locale's region, exposes `dialling`.
  `qml/pages/PrivacyPage.qml`: the country in use and the limits of phone
  matching.
- E-6: the Matrix glue of `core/src/bridges.rs` moved to the new
  `core/src/bridges/matrix.rs`, imports at the top; the test bot's banner
  comment became a doc comment.

### Bridged rooms, first-run entry

- `core/src/bridges.rs`: `Network::of_user`, `Network::of_room` (ghosts and
  bot of each bridge); test.
- `core/src/roomlist.rs` (upstream file, now marked): `bridge` and
  `contactName` on every row (`bridge_and_contact`, from heroes, direct
  targets and service members); `RoomListHandle::refresh` summarizes all rows
  again, used when the contacts index changes (`core/src/runtime.rs`).
- `src/roomlistmodel.{h,cpp}` (upstream, now marked): `bridge` and
  `contactName` roles; the name role prefers the contact's name; `bridgeOf`.
  `src/matrixbridge.{h,cpp}`: `roomBridge(roomId)`.
- `qml/pages/RoomDelegate.qml` (upstream, now marked): "Signal" / "Telegram"
  after the name. `qml/pages/RoomInfoPage.qml`: "Encrypted up to the bridge"
  and an explanation. `qml/pages/PrivacyPage.qml`: section "Signal and
  Telegram chats".
- `src/appsettings.{h,cpp}`: `bridgesIntroDone` (`bridges/introDone`).
  `qml/pages/BridgesIntroDialog.qml` (new); `qml/shipwright-shoal-messages.qml`:
  `maybeShowBridgesIntro` (once, from the home page only);
  `qml/pages/BridgesPage.qml` records it as done when opened.

### Contact names and starting chats

- `core/src/push.rs`: `push.notify` names the sender, and a direct chat, by
  the contact. `src/matrixbridge.{h,cpp}`: a woken push waits for the address
  book (at most 4 s, `ContactsBridge::settling`) before asking for the
  message; banners raised from the sync use the row's `contactName`.
- `core/src/bridges.rs`: `Request::StartChat` (`start-chat <+number>`,
  redacted in `Debug`), replies `ChatStarted`, `ChatNotFound`, `ChatFailed`
  (texts from mautrix-go v0.31.0 `bridgev2/commands/startchat.go`), `Chat`
  state in `bridge.state`, `FollowUp::JoinChat`; `room_link`; three tests.
  `core/src/bridges/matrix.rs`: joins the chat and emits `bridge.chatReady`.
  `core/src/protocol.rs`, `core/src/runtime.rs`: `bridges.startChat {bridge,
  bot?, number}`, the number completed with the contacts' dialling plan.
- `src/bridgeactions.{h,cpp}`: `startChat`, `reportChatReady`, signal
  `chatReady`. `src/contactsbridge.{h,cpp}`: `phoneBook` (name, number,
  mobile), dropped when matching goes off.
- `qml/pages/ContactPickerPage.qml`, `qml/pages/BridgedChatPage.qml` (new);
  `qml/pages/NewChatDialog.qml` (upstream, now marked): "Choose a phone
  contact"; `qml/shipwright-shoal-messages.qml`: opens the started chat over
  the home page. `shipwright-shoal-messages.pro`: the three new pages.

### Documentation

- `README.md`, `ROADMAP.md`: the above, and the device steps.

## 2026-10-01: rustfmt and clippy clean

- `core/`: `cargo fmt` over the crate, and `cargo clippy --all-targets -- -D
  warnings` (default lints) clean. Behaviour unchanged: `# Safety` sections
  on the `extern "C"` functions in `core/src/lib.rs`; `media::send` takes a
  `media::Outgoing` and `runtime.rs` `forward` a `Forward` instead of nine
  arguments; the unused `login::password_offered`, `roomlist::summarize_all`
  and `session::Paths::voice_cache` removed; small idiom fixes. Every changed
  upstream file's marker line names "rustfmt and clippy fixes".

## 2026-10-02: screenshot pass fixes

- `qml/pages/AccountPage.qml`: the Account, Device and Core rows are left
  aligned `DetailItem`s, so a long Matrix ID goes below its label at full
  width instead of breaking mid-word in half the page.
- Design pass against Sailfish's application guidelines (each changed
  upstream file's marker line names its change):
  - `qml/pages/RoomListPage.qml`: the pull-down menu holds four entries
    (Sailfish's limit); "About Shoal Messages" moved to the Account page.
    The empty-list placeholder is not shown while the header already says
    why the list is empty (unsupported server, damaged local data): the two
    drew over each other.
  - `qml/pages/AccountPage.qml`: the app's pages (Appearance, Privacy, Push
    notifications, Shipwright subscription, Signal and Telegram, Ignored
    users, About) are navigation rows (`qml/pages/NavigationItem.qml`, new)
    instead of a column of buttons, which on Sailfish read as actions.
  - `qml/pages/ConfirmDialog.qml`: the question and the explanation in the
    highlight colours (static text), the explanation at the small size.
  - `qml/pages/AppearancePage.qml`, `qml/pages/SpaceColourPage.qml`: the
    colour picker is dimmed while the automatic colour applies.
  - `qml/pages/AttachmentPickerPage.qml`: "All" and "Videos" share a path;
    only the chosen one is highlighted.
  - `qml/pages/BridgeLinkPage.qml`: a link attempt that got no answer says
    so instead of showing a bare "Try again".
  - `qml/pages/ErrorLogPage.qml`: the empty log in the style of an empty
    view (InfoLabel). `qml/pages/VerificationPage.qml`: a starting flow says
    so and shows a busy indicator instead of a blank page.
    `qml/pages/MoveToSpacePage.qml`: "No other space" only when no space is
    offered. `qml/pages/ShieldGlossaryPage.qml`: case names wrap inside the
    page margins.

## 2026-10-03: Sailfish UI rules pass

- `qml/` brought to zero findings of `tools/lint/silica-style.py` (Jolla's
  "Common pitfalls" and UI Definition of Done; each changed upstream file's
  marker line says "Sailfish UI rules pass" or names its change). Static
  labels use the highlight colours; labels in a `ListItem` or
  `BackgroundItem`, and the text of `PollBlock.qml` and `LinkPreviewCard.qml`
  (new `highlighted` property, bound to the message row), follow the press;
  colour literals and `Qt.rgba` became Theme colours (`Theme.lightPrimaryColor`
  / `darkPrimaryColor` over the camera picture, map tiles and colour-picker
  markers, `Theme.rgba` of the space colour); every flickable has a scroll
  decorator; the composer's field has a (hidden) label and the hex field a
  placeholder; small tap targets ("End poll", "What do these marks mean?",
  the shield mark) are at least `Theme.itemSizeSmall`; the image and video
  pull-downs hide until the file is there. Pull-downs hold at most four
  entries: the room's (`RoomPage.qml`) keeps Room info, Video call,
  Call / Back to the call and Load older messages; Leave or Decline, Copy
  room link, Search messages and Verify contact moved to the room's info
  page (`RoomInfoPage.qml`), New poll and Share location to the attachment
  picker's pull-down (`AttachmentPickerPage.qml`), and an invitation shows
  Decline beside Accept. A member's role changes (Make moderator, Make
  admin, Demote to member) are buttons under the role on
  `MemberProfilePage.qml`. The few findings that do not apply (QR code
  white, the picker's colour space, the unread badge on its highlight fill,
  the message body and preview tap areas, the horizontal mention strip) are
  listed with reasons in `tools/lint/silica-style-accepted.txt`.

## Upstream changes taken

None yet.

## 2026-10-03: native builds

- `scripts/sdk-env.sh` and `scripts/build-core.sh` also build the Rust core
  inside a native Sailfish OS root on an aarch64 host
  (`tools/build/native`, `SAILFISH_NATIVE=1`): the system gcc is the
  target's, so no SDK cross-compiler wrapper or sysroot is set up.

## 2026-10-08: Keel Actions for Pilot

- Pilot, the on-device assistant, reaches Shoal Messages through Keel Actions
  (ADR-0018). New files: `core/src/actions.rs` (the declarations: entity
  `conversation` with its find tool, `messages.read`, `message.send`, and the
  checks Keel's Qt 6 runtime would make: arguments and results against the
  generated schemas), `core/build.rs` (generates `actions.json` and the D-Bus
  interface file), `src/keelactions.{h,cpp}` (`org.shipwright.Keel.Actions`
  at `/org/shipwright/Keel/Actions` under the app's name, and a Qt 5
  `KeelContext` registered as `Keel.Actions 1.0`) and
  `qml/pages/ConversationContext.qml` (the room page's context: the
  conversation, its name and its last eight messages).
- Changed: `core/src/protocol.rs` and `core/src/runtime.rs` (`keel.describe`,
  `keel.invoke`, `keel.getEntity`, `keel.findEntities`; a failure's reply
  carries the Keel error `code`), `core/src/lib.rs`, `core/Cargo.toml` (the
  `keel` and `keel-actions-schema` crates, `keel-actions-codegen` to build),
  `src/matrixbridge.{h,cpp}` (routes the answers), `src/timelinemodel.{h,cpp}`
  (`transcript`), `src/shipwright-shoal-messages.cpp`, `qml/pages/RoomPage.qml`,
  the `.pro` and the spec (install `/usr/share/keel/actions/org.shipwright.ShoalMessages.json`
  and `/usr/share/dbus-1/interfaces/org.shipwright.ShoalMessages.actions.xml`),
  `org.shipwright.ShoalMessages.service` (comment: it also starts the app for
  a Keel call; Sailjail grants one name, so there is no windowless start),
  `THIRD-PARTY.md`.
