;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn assoc!
  "When applied to a transient map, adds mapping of key(s) to
  val(s). When applied to a transient vector, sets the val at index.
  Note - index must be <= (count vector). Returns coll."
  ([tcoll key val]
    (-assoc! tcoll key val))
  ([tcoll key val & kvs]
    (let [ntcoll (-assoc! tcoll key val)]
      (if kvs
        (recur ntcoll (first kvs) (second kvs) (nnext kvs))
        ntcoll))))
