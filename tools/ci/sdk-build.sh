#!/bin/bash
# SPDX-License-Identifier: LGPL-2.1-or-later
set -euo pipefail
mb2 -t "$1" build -j "$(nproc)"
