#!/bin/bash
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Pre-build hook for tools/build/sdk/build-rpms.sh (tools/build/sdk/README.md,
# "Per-spec hooks and specs.conf"): run from the repository root with the
# spec as $1. Puts the Reef repository's public signing key (not in git) where
# the installer and client specs read it. specs.conf skips both specs when
# REEF_KEY_FILE is unset.
set -euo pipefail
key=reef/installer/RPM-GPG-KEY-shipwright-reef
cp "${REEF_KEY_FILE:?REEF_KEY_FILE must point at the Reef public key}" "$key"
grep -q 'BEGIN PGP PUBLIC KEY BLOCK' "$key" ||
    { echo "$REEF_KEY_FILE is not an armoured public key" >&2; exit 1; }

# A test build for a repository other than production (e.g. one served on the
# LAN for a phone: http://<host>:8088/sailfishos/{release}/%(arch)/) sets
# REEF_URL_TEMPLATE; this edits the build's own copy of the tree, never the
# checkout (build-rpms.sh runs on a snapshot).
if [ -n "${REEF_URL_TEMPLATE:-}" ]; then
    sed -i "s|^REEF_URL_TEMPLATE=.*|REEF_URL_TEMPLATE='$REEF_URL_TEMPLATE'|" reef/installer/repo.conf
    echo "repo.conf: REEF_URL_TEMPLATE=$REEF_URL_TEMPLATE"
fi
