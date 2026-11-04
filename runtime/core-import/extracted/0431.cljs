;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype Cycle [meta all prev ^:mutable current ^:mutable _next]
  Object
  (toString [coll]
    (pr-str* coll))
  (currentval [coll]
    (when-not ^seq current
      (if-let [c (next prev)]
        (set! current c)
        (set! current all)))
    current)

  IPending
  (-realized? [coll]
    (some? current))

  IWithMeta
  (-with-meta [coll new-meta]
    (if (identical? new-meta meta)
      coll
      (Cycle. new-meta all prev current _next)))

  IMeta
  (-meta [coll] meta)

  ISeq
  (-first [coll]
    (first (.currentval coll)))
  (-rest [coll]
    (when (nil? _next)
      (set! _next (Cycle. nil all (.currentval coll) nil nil)))
    _next)

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
  (-reduce [coll f]
    (loop [s (.currentval coll) ret (first s)]
      (let [s   (or (next s) all)
            ret (f ret (first s))]
        (if (reduced? ret)
          @ret
          (recur s ret)))))
  (-reduce [coll f start]
    (loop [s (.currentval coll) ret start]
      (let [ret (f ret (first s))]
        (if (reduced? ret)
          @ret
          (recur (or (next s) all) ret))))))
