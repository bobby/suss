;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn ^number m3-hash-unencoded-chars [in]
  (let [h1 (loop [i 1 h1 m3-seed]
             (if (< i (.-length in))
               (recur (+ i 2)
                 (m3-mix-H1 h1
                   (m3-mix-K1
                     (bit-or (.charCodeAt in (dec i))
                       (bit-shift-left (.charCodeAt in i) 16)))))
               h1))
        h1 (if (== (bit-and (.-length in) 1) 1)
             (bit-xor h1 (m3-mix-K1 (.charCodeAt in (dec (.-length in)))))
             h1)]
    (m3-fmix h1 (imul 2 (.-length in)))))
