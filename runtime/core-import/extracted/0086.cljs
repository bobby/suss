;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- -lastIndexOf
  ([coll x]
   (-lastIndexOf coll x (count coll)))
  ([coll x start]
   (let [len (count coll)]
    (if (zero? len)
      -1
      (loop [idx (cond
                   (pos? start) (unchecked-min (dec len) start)
                   (neg? start) (+ len start)
                   :else start)]
        (if (>= idx 0)
          (if (= (nth coll idx) x)
            idx
            (recur (dec idx)))
          -1))))))
