;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn disj!
  "disj[oin]. Returns a transient set of the same (hashed/sorted) type, that
  does not contain key(s)."
  ([tcoll val]
    (-disjoin! tcoll val))
  ([tcoll val & vals]
    (let [ntcoll (-disjoin! tcoll val)]
      (if vals
        (recur ntcoll (first vals) (next vals))
        ntcoll))))
