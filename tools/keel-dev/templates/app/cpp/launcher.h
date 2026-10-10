// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT-0

// The app's start-up through Keel's SailfishApp library; see launcher.cpp.
#pragma once

#include <cstdint>

#include "rust/cxx.h"

std::int32_t keel_app_run(const rust::Vec<rust::String>& args);

// The app's command line from Keel's launcher (Keel::launchArguments()):
// when booster-keel runs the app, std::env::args() holds the booster's.
rust::Vec<rust::String> keel_launch_arguments();
