;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- equiv-sequential
  "Assumes x is sequential. Returns true if x equals y, otherwise
  returns false."
  [x y]
  (boolean
    (when (sequential? y)
      (if (and (counted? x) (counted? y)
               (not (== (count x) (count y))))
        false
        (loop [xs (seq x) ys (seq y)]
          (cond (nil? xs) (nil? ys)
            (nil? ys) false
            (= (first xs) (first ys)) (recur (next xs) (next ys))
            :else false))))))
