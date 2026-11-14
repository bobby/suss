;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- array-copy
  ([from i to j len]
    (loop [i i j j len len]
       (if (zero? len)
         to
         (do (aset to j (aget from i))
             (recur (inc i) (inc j) (dec len)))))))
