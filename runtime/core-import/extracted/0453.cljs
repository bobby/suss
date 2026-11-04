;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn take
  "Returns a lazy sequence of the first n items in coll, or all items if
  there are fewer than n.  Returns a stateful transducer when
  no collection is provided."
  ([n]
   {:pre [(number? n)]}
     (fn [rf]
       (let [na (volatile! n)]
         (fn
           ([] (rf))
           ([result] (rf result))
           ([result input]
              (let [n @na
                    nn (vswap! na dec)
                    result (if (pos? n)
                             (rf result input)
                             result)]
                (if (not (pos? nn))
                  (ensure-reduced result)
                  result)))))))
  ([n coll]
   {:pre [(number? n)]}
     (lazy-seq
       (when (pos? n)
         (when-let [s (seq coll)]
           (cons (first s) (take (dec n) (rest s))))))))
