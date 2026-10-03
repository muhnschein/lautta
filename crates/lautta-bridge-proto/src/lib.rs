// SPDX-License-Identifier: LGPL-2.1-or-later
//! netvfs bridge protocol `org.netvfs.Bridge1` (netvfs SPEC-v2 §8a): wire
//! types, the client proxy, and (feature `fake`) an in-process fake bridge
//! that replays the shared contract sequences (SPEC TST-2).
#![forbid(unsafe_code)]
