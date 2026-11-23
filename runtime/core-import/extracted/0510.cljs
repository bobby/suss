;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn sort
  "Returns a sorted sequence of the items in coll. Comp can be
   boolean-valued comparison function, or a -/0/+ valued comparator.
   Comp defaults to compare."
  ([coll]
   (sort compare coll))
  ([comp coll]
   (if (seq coll)
     (let [a (to-array coll)]
       ;; matching Clojure's stable sort, though docs don't promise it
       (garray/stableSort a (fn->comparator comp))
       (with-meta (seq a) (meta coll)))
     ())))
