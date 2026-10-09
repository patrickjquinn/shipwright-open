# Upstream

<!-- SPDX-FileCopyrightText: 2026 Patrick Quinn -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

| | |
|---|---|
| Project | xmatic (harbour-xmatic), by JimKnopfIoT |
| URL | https://github.com/JimKnopfIoT/harbour-xmatic |
| Licence | Apache-2.0 (`LICENSE`, byte-identical to the canonical apache.org text) |
| Imported | tag `v0.41.0`, commit `40ce16b482d5a8afe87e0dfc5e6910994a5c2f1a` (2026-09-30) |
| Imported on | 2026-09-30, by `git archive` (no `.git`, no history) |
| Upstream NOTICE | none shipped; ours is `NOTICE` |

## Why v0.41.0 and not v0.34.1

The plan pins v0.34.1 (released 2026-09-17). On 2026-09-30 the upstream
repository has **no `v0.34.1` tag**; its only tag is `v0.41.0`, which is also
the latest commit. Per the import instructions (tag, or latest commit if the tag
is missing) the fork is based on the latest commit, `v0.41.0`.

The 0.34.1 release still exists as an untagged commit:
`e7274f2e0206bfc012b03a5ea959b5e3c6a978a5` ("harbour-xmatic 0.34.1 — an
invitation shows the invitation, not the room before it", 2026-09-17). Between
the two lie seven releases (0.35 to 0.41), including the move to
matrix-rust-sdk 0.19, which converts local data on first start. Rebasing onto
0.34.1 would mean redoing the rebrand on that tree; nothing in this fork depends
on post-0.34.1 features.

## Cherry-picking from upstream

Nothing here tracks upstream automatically (see README, "Fork policy"). To take
an upstream change:

```sh
git clone https://github.com/JimKnopfIoT/harbour-xmatic /tmp/xmatic
cd /tmp/xmatic && git log --oneline 40ce16b..origin/main   # what is new
git format-patch -1 <commit> -o /tmp/patches
cd <monorepo> && git am -3 --directory=shoal/messages /tmp/patches/*.patch
```

Expect conflicts on renamed files and identifiers; the mapping is in
`CHANGES-FROM-UPSTREAM.md`. Record each pick in that file under "Upstream
changes taken".
