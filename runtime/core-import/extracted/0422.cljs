;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn range
  "Returns a lazy seq of nums from start (inclusive) to end
   (exclusive), by step, where start defaults to 0, step to 1,
   and end to infinity."
  ([] (range 0 (.-MAX_VALUE js/Number) 1))
  ([end] (range 0 end 1))
  ([start end] (range start end 1))
  ([start end step]
   (cond
     (pos? step)
     (if (<= end start)
       ()
       (if (and (integer? start) (integer? end) (integer? step))
         (IntegerRange. nil start end step (range-count start end step) nil)
         (Range. nil start end step nil nil nil)))

     (neg? step)
     (if (>= end start)
       ()
       (if (and (integer? start) (integer? end) (integer? step))
         (IntegerRange. nil start end step (range-count start end step) nil)
         (Range. nil start end step nil nil nil)))

     :else
     (if (== end start)
       ()
       (repeat start)))))
