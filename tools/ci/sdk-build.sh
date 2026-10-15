#!/bin/bash
# SPDX-License-Identifier: LGPL-2.1-or-later
# Runs inside the SDK container: mb2 build of the RPM. The binary is already
# in prebuilt/ (tools/ci/cross-build.sh), so mb2 only packages it with the
# QML, translations and icons.
set -euo pipefail
mb2 -t "$1" build -j "$(nproc)"
