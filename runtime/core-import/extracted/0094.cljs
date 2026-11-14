;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn ^number hash-ordered-coll
  "Returns the hash code, consistent with =, for an external ordered
   collection implementing Iterable.
   See http://clojure.org/data_structures#hash for full algorithms."
  [coll]
  (loop [n 0 hash-code 1 coll (seq coll)]
    (if-not (nil? coll)
      (recur (inc n) (bit-or (+ (imul 31 hash-code) (hash (first coll))) 0)
        (next coll))
      (mix-collection-hash hash-code n))))
