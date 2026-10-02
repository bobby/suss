;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn conj
  "conj[oin]. Returns a new collection with the xs
  'added'. (conj nil item) returns (item).
  (conj coll) returns coll. (conj) returns [].
  The 'addition' may happen at different 'places' depending
  on the concrete type."
  ([] [])
  ([coll] coll)
  ([coll x]
    (if-not (nil? coll)
      (-conj coll x)
      (list x)))
  ([coll x & xs]
    (if xs
      (recur (conj coll x) (first xs) (next xs))
      (conj coll x))))
