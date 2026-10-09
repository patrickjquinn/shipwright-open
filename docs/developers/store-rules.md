<!--
SPDX-FileCopyrightText: 2026 Patrick Quinn
SPDX-License-Identifier: CC-BY-4.0
-->

# Reef store rules

**DRAFT. Requires legal review before publication.** These rules become part of the developer terms (terms version `draft-2026-09`, which sign-up must accept). The technical rules are enforced by the automated review; review-checklist.md lists every check.

## Accounts

**No registration fee.** Opening a developer account, keeping it, uploading builds and having them reviewed and published cost nothing, for free and paid apps alike. Reef takes no share of sales: for now, developers of paid apps sell them through their own checkout, and Reef lists them free of charge. A revenue-share programme may follow; pricing-and-revenue-share.md keeps its draft. *Decided; still subject to the legal review above.*

1. One account per developer or organisation. The e-mail address must be verified before an API key is issued.
2. API keys are secrets. Keep them in CI secret stores, revoke them (`DELETE /v1/me/api-keys/{id}`) when a machine or person leaves, and never put them in an app or a public repository. Keys start with `swdp_` so secret scanners can find them.
3. You sign every package with your own OpenPGP key, registered with `POST /v1/me/signing-keys`. Reef re-signs published packages with the Reef repository key; your signature proves to us that the upload is yours.

## App ids and names

4. The app id is the RPM package name, the binary name, the desktop file name and the licence `app` claim. It is 3 to 64 characters of lowercase letters, digits and single hyphens, starts with a letter and does not end with a hyphen.
5. Reserved and refused:
   - `harbour-` (Jolla's store namespace: a Reef package must never shadow a Harbour package);
   - Shipwright and platform prefixes: `shipwright`, `shoal-`, `reef-`, `keel-`, `sailfish`, `jolla`, `nemo-`, `lipstick`, `mapplauncherd`, `sailjail`, `ssu`, `qt5`, `qt6`, `chum`, `patchmanager`, `store-client`;
   - the names `shoal`, `reef`, `keel`, `harbour`, `store`, `system`, and the names of OS packages;
   - subpackage-like suffixes: `-devel`, `-debuginfo`, `-debugsource`, `-tests`.
6. App ids are first come, first served, but we may reassign one that impersonates another product or trademark. The display title must not imply endorsement by Jolla, Shipwright or anyone else.

## Packages

7. One binary RPM per upload, built for the release and architecture you upload it for (or `noarch`). No source RPMs.
8. Every version you publish for a release and architecture is newer, by rpm's version comparison, than the last one we accepted there.
9. Files stay inside your app's paths. No setuid or setgid files, no world-writable files, no file capabilities, all files owned by `root:root`.
10. Scriptlets run as root outside any sandbox, so only the cache refreshes on the allow-list are accepted. No `%pretrans`, no triggers, no file triggers.
11. You may not `Provide` anything but your own package, its desktop and metainfo, or MIME handlers, and you may not `Obsolete` anything.
12. Upload limit: 100 MiB per RPM (installed size 512 MiB), unless we agree otherwise.

## Sandbox and permissions

13. Every app declares Sailjail permissions in `[X-Sailjail]`. The store shows them to users before installing.
14. `Sandboxing=Disabled` is refused unless a reviewer approves it after you explain why the app cannot work sandboxed (file managers, system tools). The listing marks the app as unsandboxed.
15. `Privileged` and `ApplicationInstallation` need a reviewer's approval and a justification.
16. Apps do not download and execute code that was not reviewed (plugins, scripts, updaters). Updates come through Reef.

## Content and behaviour

17. The app does what its listing says. No malware, spyware, cryptocurrency mining, ad fraud or hidden functionality. Every upload passes the malware scan.
18. Collecting personal data needs a privacy policy linked from the listing and the user's informed consent where the law requires it. Health, location and contact data are sensitive.
19. No content that is illegal in the EU, and no content in the categories the final terms exclude (to be set by legal review).
20. Third-party code is used under its licence. The RPM `License` tag is accurate, and GPL apps provide their source (`source` URL on the app).

## Enforcement

21. We may reject a build with reasons, withdraw a published build (for security, legal or policy reasons), suspend an account, or remove an app. Withdrawn builds leave the repository at the next publish run; users who installed them keep them unless we push a security update.
22. You can ask a different reviewer to look at a rejection again. The appeal process is to be defined in the final terms.
