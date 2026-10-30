#!/bin/bash
# SPDX-License-Identifier: LGPL-2.1-or-later
# Inside the SDK container. Binary already in prebuilt/; mb2 only packages.
set -euo pipefail
mb2 -t "$1" build -j "$(nproc)"
