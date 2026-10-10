#!/bin/sh
# SPDX-FileCopyrightText: 2026 Patrick Quinn
# SPDX-License-Identifier: MIT-0
#
# Renders icons/{{name}}.svg to the Sailfish launcher sizes,
# icons/hicolor/<N>x<N>/apps/{{name}}.png, which the spec installs. Commit
# the PNGs, so packaging needs no SVG renderer. Rerun after editing the SVG.
#
#   icons/render-icons.sh        # needs rsvg-convert (librsvg2-bin)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
command -v rsvg-convert >/dev/null 2>&1 || {
    echo "render-icons.sh: rsvg-convert not found (Debian/Ubuntu: librsvg2-bin; Fedora: librsvg2-tools)" >&2
    exit 2
}
for n in 86 108 128 172; do
    out="$here/hicolor/${n}x${n}/apps/{{name}}.png"
    mkdir -p "$(dirname "$out")"
    rsvg-convert -w "$n" -h "$n" "$here/{{name}}.svg" -o "$out"
    echo "$out"
done
