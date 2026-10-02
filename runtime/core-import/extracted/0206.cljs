;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn dissoc!
  "Returns a transient map that doesn't contain a mapping for key(s)."
  ([tcoll key]
    (-dissoc! tcoll key))
  ([tcoll key & ks]
    (let [ntcoll (-dissoc! tcoll key)]
      (if ks
        (recur ntcoll (first ks) (next ks))
        ntcoll))))
