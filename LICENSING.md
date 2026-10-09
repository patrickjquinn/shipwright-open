<!--
SPDX-FileCopyrightText: 2026 Patrick Quinn
SPDX-License-Identifier: CC-BY-4.0
-->

# Licensing

Shipwright is a mix of open-source and proprietary parts. The parts people must trust or build on are open; the paid apps and the hosted services are not. Every file states its licence in an SPDX header or through `REUSE.toml`, and `reuse lint` checks that nothing is missing. The names and logos are covered by [TRADEMARKS.md](TRADEMARKS.md), not by any of these licences.

| Component | Licence | Notes |
| --- | --- | --- |
| Keel (`keel/`) | MIT | Paid and GPL apps can both link it. The ported Silica QML keeps BSD-3-Clause (`keel/silica/PROVENANCE.md`); keel-shell's upstream parts keep BSD-3-Clause; the Nemo compatibility libraries keep LGPL-2.1-only (`keel/nemo-compat/PROVENANCE.md`) and stay separate shared libraries. |
| Reef installer and Reef app (`reef/installer/`, `reef/client/`, `reef/rpm/`) | GPL-3.0-or-later | They decide what gets installed on a phone, so anyone can audit and rebuild them. |
| Licence check and secrets libraries (`reef/licence/`, `shoal/secrets/`) | MIT OR Apache-2.0 | Linked into both paid and GPL apps. |
| Shoal Keys (`shoal/keys/`) | GPL-3.0-or-later | Free, with an optional paid supporter licence. |
| Shoal Messages (`shoal/messages/`) | Apache-2.0 | A fork of harbour-xmatic, which is Apache-2.0. |
| AirPods daemon (`shoal/bridge-airpods/daemon/`, `shoal/bridge-airpods/rpm/shipwright-shoal-bridge-airpods.spec`, `shoal/bridge-airpods/rpm/make-sources.sh`) | GPL-3.0-only | Its own licence, as before, including its build and vendoring scripts and tests; the paid UI talks to it over D-Bus. `daemon/tests/tst_ui_link.qml` loads the paid UI and stays proprietary. |
| Keel launcher (`tools/keel-launcher/`) | MIT | `include!`d into the GPL and paid apps at build time, so it carries Keel's licence. |
| Build scripts for the open packages (`tools/build/sdk/`, `tools/build/native/`, `rust-toolchain.toml`, `shoal/messages/rpm/prebuild.sh`) | MIT (the Messages hook: Apache-2.0) | The scripts that control compiling and packaging the GPL apps belong to their corresponding source. |
| This file and `TRADEMARKS.md` | CC-BY-4.0 | |
| Developer documentation (`docs/developers/`) | CC-BY-4.0 | |
| The Reef and Shoal Keys app icons (`icons/hicolor/*/apps/shipwright-reef.png`, `shipwright-shoal-keys.png`) | CC-BY-4.0 | The names and logos remain trademarks (`TRADEMARKS.md`). |
| Paid apps (Shoal Bridge UI, Mail, Camera, Pilot and the rest), hosted services, the other tools (including the `keel` developer CLI in `tools/keel-dev/`), the other app icons and everything else | LicenseRef-Shipwright-Proprietary | All rights reserved. |

Third-party code keeps its own licence; see `LICENSES/` and the `PROVENANCE.md` and `upstream-licenses/` files next to it. Apps under a proprietary licence that load the LGPL Nemo compatibility libraries allow reverse engineering of those libraries for debugging, as LGPL-2.1 section 6 requires.

The hosted Matrix stack runs Synapse and the mautrix bridges unmodified under their AGPL-3.0 licences; a modified version would have to offer its source to the service's users.

The copyright holder for everything not marked otherwise is Patrick Quinn, trading as Shipwright.

## Public source

The open components above are published, one way, to the public repository `shipwright-open`: `tools/opensource/export.sh` builds it from a commit of this repository using the path list in `tools/opensource/paths.txt`, refuses any file that is proprietary or unlicensed, and checks the result with `reuse lint`. Each export is one commit there, tagged like the release it accompanies. Nobody commits to the public repository directly; contributions made there are applied here and come back with the next export (`tools/opensource/README.md`).
