;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype IntegerRange [meta start end step cnt ^:mutable __hash]
  Object
  (toString [coll]
    (pr-str* coll))
  (equiv [this other]
    (-equiv this other))
  (indexOf [coll x]
    (-indexOf coll x 0))
  (indexOf [coll x start]
    (-indexOf coll x start))
  (lastIndexOf [coll x]
    (-lastIndexOf coll x (count coll)))
  (lastIndexOf [coll x start]
    (-lastIndexOf coll x start))

  ICloneable
  (-clone [_] (IntegerRange. meta start end step cnt __hash))

  IWithMeta
  (-with-meta [rng new-meta]
    (if (identical? new-meta meta)
      rng
      (IntegerRange. new-meta start end step cnt __hash)))

  IMeta
  (-meta [rng] meta)

  ISeqable
  (-seq [rng] rng)

  ISeq
  (-first [rng] start)
  (-rest [rng]
    (let [s (-next rng)]
      (if (nil? s)
        ()
        s)))

  IIterable
  (-iterator [_]
    (RangeIterator. start end step))

  INext
  (-next [rng]
    (if (pos? step)
      (when (< (+ start step) end)
        (IntegerRange. nil (+ start step) end step (range-count (+ start step) end step) nil))
      (when (> (+ start step) end)
        (IntegerRange. nil (+ start step) end step (range-count (+ start step) end step) nil))))

  IDrop
  (-drop [rng n]
    (if (pos? n)
      (if (< n cnt)
        (IntegerRange. nil (+ start (* step n)) end step (- cnt n) nil)
        nil)
      rng))

  IChunkedSeq
  (-chunked-first [rng]
    (IntegerRangeChunk. start step (unchecked-min cnt 32)))
  (-chunked-rest [rng]
    (if (<= cnt 32)
      ()
      (let [start (+ start (* step 32))]
        (cond
          (pos? step)
          (if (<= end start)
            ()
            (IntegerRange. nil start end step (range-count start end step) nil))

          (neg? step)
          (if (>= end start)
            ()
            (IntegerRange. nil start end step (range-count start end step) nil))

          :else
          (if (== end start)
            ()
            (repeat start))))))

  IChunkedNext
  (-chunked-next [rng]
    (seq (-chunked-rest rng)))

  ICollection
  (-conj [rng o] (cons o rng))

  IEmptyableCollection
  (-empty [rng] (.-EMPTY List))

  ISequential
  IEquiv
  (-equiv [rng other] (equiv-sequential rng other))

  IHash
  (-hash [rng] (caching-hash rng hash-ordered-coll __hash))

  ICounted
  (-count [rng]
    cnt)

  IIndexed
  (-nth [rng n]
    (if (and (<= 0 n) (< n (-count rng)))
      (+ start (* n step))
      (if (and (<= 0 n) (> start end) (zero? step))
        start
        (throw (js/Error. "Index out of bounds")))))
  (-nth [rng n not-found]
    (if (and (<= 0 n) (< n (-count rng)))
      (+ start (* n step))
      (if (and (<= 0 n) (> start end) (zero? step))
        start
        not-found)))

  IReduce
  (-reduce [rng f] (ci-reduce rng f))
  (-reduce [rng f init]
    (loop [i start ret init]
      (if (if (pos? step) (< i end) (> i end))
        (let [ret (f ret i)]
          (if (reduced? ret)
            @ret
            (recur (+ i step) ret)))
        ret))))
