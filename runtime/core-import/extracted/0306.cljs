;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn empty?
  "Returns true if coll has no items. To check the emptiness of a seq,
  please use the idiom (seq x) rather than (not (empty? x))"
  [coll]
  (cond
    (nil? coll)
    true

    (satisfies? ICounted coll)
    (zero? (-count coll))

    :else
    (not (seq coll))))
