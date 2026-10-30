;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype Iterate [meta f prev-seed ^:mutable seed ^:mutable next]
  Object
  (toString [coll]
    (pr-str* coll))

  IPending
  (-realized? [coll]
    (not (identical? seed UNREALIZED-SEED)))

  IWithMeta
  (-with-meta [coll new-meta]
    (if (identical? new-meta meta)
      coll
      (Iterate. new-meta f prev-seed seed next)))

  IMeta
  (-meta [coll] meta)

  ISeq
  (-first [coll]
    (when (identical? UNREALIZED-SEED seed)
      (set! seed (f prev-seed)))
    seed)
  (-rest [coll]
    (when (nil? next)
      (set! next (Iterate. nil f (-first coll) UNREALIZED-SEED nil)))
    next)

  INext
  (-next [coll]
    (-rest coll))

  ICollection
  (-conj [coll o] (cons o coll))

  IEmptyableCollection
  (-empty [coll] (.-EMPTY List))

  ISequential
  ISeqable
  (-seq [coll] coll)

  IReduce
  (-reduce [coll rf]
    (let [first (-first coll)
          v     (f first)]
      (loop [ret (rf first v) v v]
        (if (reduced? ret)
          @ret
          (let [v (f v)]
            (recur (rf ret v) v))))))
  (-reduce [coll rf start]
    (let [v (-first coll)]
      (loop [ret (rf start v) v v]
        (if (reduced? ret)
          @ret
          (let [v (f v)]
            (recur (rf ret v) v)))))))
