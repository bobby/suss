;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn take-while
  "Returns a lazy sequence of successive items from coll while
  (pred item) returns logical true. pred must be free of side-effects.
  Returns a transducer when no collection is provided."
  ([pred]
     (fn [rf]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
            (if (pred input)
              (rf result input)
              (reduced result))))))
  ([pred coll]
     (lazy-seq
       (when-let [s (seq coll)]
         (when (pred (first s))
           (cons (first s) (take-while pred (rest s))))))))
