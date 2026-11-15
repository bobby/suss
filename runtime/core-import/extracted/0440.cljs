;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn take-nth
  "Returns a lazy seq of every nth item in coll.  Returns a stateful
  transducer when no collection is provided."
  ([n]
   {:pre [(number? n)]}
     (fn [rf]
       (let [ia (volatile! -1)]
         (fn
           ([] (rf))
           ([result] (rf result))
           ([result input]
              (let [i (vswap! ia inc)]
                (if (zero? (rem i n))
                  (rf result input)
                  result)))))))
  ([n coll]
   {:pre [(number? n)]}
     (lazy-seq
       (when-let [s (seq coll)]
         (cons (first s) (take-nth n (drop n s)))))))
