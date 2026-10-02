;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- pv-reduce
  ([pv f start end]
   (if (< start end)
     (pv-reduce pv f (nth pv start) (inc start) end)
     (f)))
  ([pv f init start end]
   (loop [acc init i start arr (unchecked-array-for pv start)]
     (if (< i end)
       (let [j (bit-and i 0x01f)
             arr (if (zero? j) (unchecked-array-for pv i) arr)
             nacc (f acc (aget arr j))]
         (if (reduced? nacc)
           @nacc
           (recur nacc (inc i) arr)))
       acc))))
