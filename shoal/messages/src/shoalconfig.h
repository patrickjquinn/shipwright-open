// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// Shoal Messages: the one place Shipwright's service endpoints are configured.
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
//
// Each value can be overridden at build time without editing this file, e.g.
//   %qmake5 "DEFINES+=SHOAL_DEFAULT_PUSH_GATEWAY=\\\"https://push.example.net/_matrix/push/v1/notify\\\""
#ifndef SHOALCONFIG_H
#define SHOALCONFIG_H

// The Matrix push gateway the homeserver posts to when the user has not set
// one: Shipwright's hosted ntfy, which serves the Matrix push gateway API at
// /_matrix/push/v1/notify. PLACEHOLDER host until services/ deploys it.
#ifndef SHOAL_DEFAULT_PUSH_GATEWAY
#define SHOAL_DEFAULT_PUSH_GATEWAY "https://push.shipwright.example/_matrix/push/v1/notify"
#endif

// The preferred UnifiedPush distributor (Shoal Push) is chosen in the Rust core,
// core/src/push.rs (SHOAL_DISTRIBUTOR), because that is where registration
// happens; it is repeated nowhere else.

#endif // SHOALCONFIG_H
