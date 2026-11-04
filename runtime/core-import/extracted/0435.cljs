;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype IntegerRangeChunk [start step count]
  ICounted
  (-count [coll] count)

  ISeq
  (-first [coll] start)

  IIndexed
  (-nth [coll i]
    (+ start (* i step)))
  (-nth [coll i not-found]
    (if (and (>= i 0) (< i count))
      (+ start (* i step))
      not-found))

  IChunk
  (-drop-first [coll]
    (if (<= count 1)
      (throw (js/Error. "-drop-first of empty chunk"))
      (IntegerRangeChunk. (+ start step) step (dec count)))))
