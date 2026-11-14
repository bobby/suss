;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- pam-grow-seed-array [seed trailing]
  (let [seed-cnt  (dec (alength seed))
        extra-kvs (seq trailing)
        ret       (make-array (+ seed-cnt (* 2 (count extra-kvs))))
        ret       (array-copy seed 0 ret 0 seed-cnt)]
    (loop [i seed-cnt extra-kvs extra-kvs]
      (if extra-kvs
        (let [kv (first extra-kvs)]
          (aset ret i (-key kv))
          (aset ret (inc i) (-val kv))
          (recur (+ 2 i) (next extra-kvs)))
        ret))))
