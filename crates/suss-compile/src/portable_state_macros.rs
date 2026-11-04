// Copyright (c) Rich Hickey. All rights reserved.
// Eclipse Public License 1.0; retain the upstream notice and runtime/core-import/epl-v10.html.
// SPDX-License-Identifier: EPL-1.0
// Pinned cljs/core.cljc delay (line 2293) and vswap! (line 3021) macros (#30).
// Only the declaration heads are normalized, and delay passes the Delay
// type's added exception field; see docs/compatibility/state/README.md.
pub(crate) const DELAY: &str = include_str!("../../../docs/compatibility/state/delay.sus");
pub(crate) const VSWAP: &str = include_str!("../../../docs/compatibility/state/vswap.sus");
