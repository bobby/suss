;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn equiv-map
  "Test map equivalence. Returns true if x equals y, otherwise returns false."
  [x y]
  (boolean
    (when (and (map? y) (not (record? y)))
      ; assume all maps are counted
      (when (== (count x) (count y))
        (if (satisfies? IKVReduce x)
          (reduce-kv
            (fn [_ k v]
              (if (= (get y k never-equiv) v)
                true
                (reduced false)))
            true x)
          (every?
            (fn [xkv]
              (= (get y (first xkv) never-equiv) (second xkv)))
            x))))))
