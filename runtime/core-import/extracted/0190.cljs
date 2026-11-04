;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- array-extend-kv [arr k v]
  (let [l (alength arr)
        narr (make-array (+ l 2))]
    (loop [i 0]
      (when (< i l)
        (aset narr i (aget arr i))
        (recur (inc i))))
    (aset narr l k)
    (aset narr (inc l) v)
    narr))
