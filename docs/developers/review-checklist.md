<!--
SPDX-FileCopyrightText: 2026 Patrick Quinn
SPDX-License-Identifier: CC-BY-4.0
-->

# Review checklist

Every upload goes through the automated checks below, in this order. Each check ends as `pass`, `warn`, `manual`, `fail` or `skipped`. Any `fail` rejects the upload (HTTP 422, nothing but the report is kept). A `manual` check must be acknowledged by name when a reviewer approves the build. Then a human reviewer runs the manual part.

## Automated

| Check id | Fails when | Notes |
| --- | --- | --- |
| `size` | The upload exceeds the upload limit (default 100 MiB) | Oversized uploads are cut off while streaming with HTTP 413 and never reach review |
| `rpm.header` | Not a binary RPM (bad lead magic), a source RPM, or rpm cannot read the header | The lead is checked before rpm sees the file |
| `signature` | No registered signing key, or the RPM is not signed by one of the developer's registered keys | Checked with `rpmkeys --checksig` in a scratch rpmdb holding only those keys |
| `rpm.metadata` | Name differs from the app id; an Epoch; version characters rpm does not allow; arch is neither the upload's arch nor `noarch`; no SOURCERPM; empty or over-long Summary, empty description or License; installed size over the limit (default 512 MiB); any Obsoletes; a Provides other than the app, its desktop/metainfo or MIME handlers; the desktop file missing; any file outside the app's paths; a device, FIFO or socket; setuid/setgid; world-writable; not `root:root`; file capabilities; version-release not newer than the last accepted build for this release and arch | All problems are listed in `details` |
| `rpm.conflicts` | never (warn) | Conflicts are shown to the reviewer |
| `scriptlets` | `%pretrans`; any trigger, file trigger or `%verifyscript`; an interpreter other than `/bin/sh` (or a bare `ldconfig`); any line not on the allow-list | Allow-list: `update-desktop-database [-q] [/usr/share/applications]`, `gtk-update-icon-cache [-q -t -f] /usr/share/icons/hicolor`, `touch --no-create /usr/share/icons/hicolor`, `ldconfig`, `:`, `true`, `exit 0`, each optionally followed by `|| :`, `|| true` or output redirection to `/dev/null`. Shell metacharacters anywhere else fail the line |
| `payload` | The payload cannot be decompressed within limits (1 GiB decompressed, 20,000 entries, 2 MiB per reviewed file, 64 MiB reviewed in total), unsafe paths, or a timeout | Only the desktop file, the Keel Actions files and `.qml`, `.js`, `qmldir` under `/usr/share` are extracted |
| `sailjail` | Desktop file missing or malformed, `Type` not `Application`, no `Name`/`Exec`, no `[X-Sailjail]`, no `OrganizationName`/`ApplicationName`/`Permissions`, an unknown permission, or `Sandboxing` other than `Enabled`/`Disabled` | Permissions are copied to the catalogue |
| `sailjail.sandboxing_disabled` | manual | `Sandboxing=Disabled`. The reviewer approves only with a written justification from the developer |
| `sailjail.sensitive_permissions` | manual | `Privileged` or `ApplicationInstallation` |
| `keel.actions` | A Keel Actions file is for an app ID other than the desktop file's `OrganizationName.ApplicationName`; the manifest breaks a review rule (a tool, entity type or prompt without a description of 20 to 1,024 characters; an action whose name or description says delete, remove, erase, wipe, overwrite, send, pay or purchase without `destructiveHint` or a `notDestructiveBecause`; a string without `maxLength` up to 1,000,000, an array without `maxItems` up to 10,000, an integer without bounds, an object without `additionalProperties: false`, nesting deeper than 4; a tool or prompt outside the app's namespace; an entity reference to an undeclared type); the activation file has anything but `Name=<app ID>` and `Exec=/usr/bin/sailjail -p <app>.desktop /usr/bin/<app> --keel-actions` | `skipped` without Keel Actions; `warn` when `[X-Sailjail]` lacks `ExecDBus=/usr/bin/<app> --keel-actions`. Developers run the same rules with `keel actions --review`. The `org.shipwright` namespace is only for Shipwright's own publisher accounts (granted by an operator, never by app name) |
| `keel_compat` | never (warn or pass) | `keel-compat --json` on the extracted QML. The tier reached (`B`, `A`, `none`, `not-applicable`) and blockers are recorded (`keel_tier` in store metadata). The listing shows it as a Keel grade: tier B is grade A (everything the app uses from Silica runs on Keel), tier A is grade B, `none` is "Not supported yet". `skipped` when not configured |
| `malware` | The scanner reports a detection (exit 1) | Any other scanner failure becomes `manual`. `warn` when no scanner is configured |

## Manual (reviewer)

Before approving, the reviewer:

1. Reads the report. Every `warn` has an explanation or is harmless; every `manual` item is justified in the developer's notes.
2. Checks the listing: title, category, homepage and source match the app; nothing impersonates another product (store rules 5 and 6).
3. Installs the build on a test device for **each release it will be listed for** (the `tested_on` list, which the reviewer can narrow when approving), launches it from the app grid as users do (directly into Lipstick, through Keel's booster when installed; keel-shell only for apps that declare it as their fallback), and checks:
   - the app starts, the cover appears, and the app closes cleanly;
   - it asks for no permission beyond those declared;
   - with network, it talks only to the services the listing implies.
4. For paid apps: the app refuses to unlock without a licence and unlocks with a test licence for its app id.
5. For an update: the diff of permissions, files and scriptlets against the published build (both reports are available at `GET /v1/review/builds/{id}`).
6. Approves with `POST /v1/review/builds/{id}/approve`, listing every manual check id in `acknowledge` and a note, or rejects with a reason.

Approval publishes the build into the staging tree: the RPM goes to `sailfishos/<release>/<arch>/`, replacing the previously published build for that app, release and arch, and `store-meta.json` gets the catalogue entry with its tested-on releases. The next repository publish run re-signs it with the Reef key and generates the repository and catalogue.

## Withdrawal

`POST /v1/review/builds/{id}/withdraw` with a reason removes a published build from staging and from the store metadata at once. Decide separately whether to ask the developer for a fixed build.
