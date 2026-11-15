;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn drop
  "Returns a laziness-preserving sequence of all but the first n items in coll.
  Returns a stateful transducer when no collection is provided."
  ([n]
   {:pre [(number? n)]}
     (fn [rf]
       (let [na (volatile! n)]
         (fn
           ([] (rf))
           ([result] (rf result))
           ([result input]
              (let [n @na]
                (vswap! na dec)
                (if (pos? n)
                  result
                  (rf result input))))))))
  ([n coll]
   {:pre [(number? n)]}
     (if (implements? IDrop coll)
       (or
         (if (pos? n)
           (-drop coll (Math/ceil n))
           (seq coll))
         ())
       (let [step (fn [n coll]
                    (let [s (seq coll)]
                      (if (and (pos? n) s)
                        (recur (dec n) (rest s))
                        s)))]
         (lazy-seq (step n coll))))))
