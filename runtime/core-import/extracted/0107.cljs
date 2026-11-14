;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defprotocol IWatchable
  "Protocol for types that can be watched. Currently only implemented by Atom."
  (-notify-watches [this oldval newval]
    "Calls all watchers with this, oldval and newval.")
  (-add-watch [this key f]
    "Adds a watcher function f to this. Keys must be unique per reference,
     and can be used to remove the watch with -remove-watch.")
  (-remove-watch [this key]
    "Removes watcher that corresponds to key from this."))
