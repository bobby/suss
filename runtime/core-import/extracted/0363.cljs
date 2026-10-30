;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn ^:private compare-indexed
  "Compare indexed collection."
  ([xs ys]
     (let [xl (count xs)
           yl (count ys)]
       (cond
        (< xl yl) -1
        (> xl yl) 1
        (== xl 0) 0
        :else (compare-indexed xs ys xl 0))))
  ([xs ys len n]
     (let [d (compare (nth xs n) (nth ys n))]
       (if (and (zero? d) (< (+ n 1) len))
         (recur xs ys len (inc n))
         d))))
