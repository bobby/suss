;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype Range [meta start end step ^:mutable chunk ^:mutable chunk-next ^:mutable __hash]
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
  (forceChunk [coll]
    (when (nil? chunk)
      (let [arr (make-array 32)
            val (loop [n 0 val start]
                  (if (< n 32)
                    (do
                      (aset arr n val)
                      (let [n (inc n)
                            val (+ val step)]
                        (if (if (pos? step) (< val end) (> val end))
                          (recur n val)
                          (set! chunk (array-chunk arr 0 n)))))
                    val))]
        (when (nil? chunk)
          (set! chunk (array-chunk arr 0 32))
          (when (if (pos? step) (< val end) (> val end))
            (set! chunk-next (Range. nil val end step nil nil nil)))))))

  ICloneable
  (-clone [_] (Range. meta start end step chunk chunk-next __hash))

  IWithMeta
  (-with-meta [rng new-meta]
    (if (identical? new-meta meta)
      rng
      (Range. new-meta start end step chunk chunk-next __hash)))

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
        (Range. nil (+ start step) end step nil nil nil))
      (when (> (+ start step) end)
        (Range. nil (+ start step) end step nil nil nil))))

  IChunkedSeq
  (-chunked-first [rng]
    (.forceChunk rng)
    chunk)
  (-chunked-rest [rng]
    (.forceChunk rng)
    (if (nil? chunk-next)
      ()
      chunk-next))

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

  IReduce
  (-reduce [rng f] (seq-reduce f rng))
  (-reduce [rng f init]
    (loop [i start ret init]
      (if (if (pos? step) (< i end) (> i end))
        (let [ret (f ret i)]
          (if (reduced? ret)
            @ret
            (recur (+ i step) ret)))
        ret))))
