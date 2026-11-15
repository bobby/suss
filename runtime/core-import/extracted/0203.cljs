;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype ChunkedSeq [vec node i off meta ^:mutable __hash]
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

  IWithMeta
  (-with-meta [coll new-meta]
    (if (identical? new-meta meta)
      coll
      (chunked-seq vec node i off new-meta)))
  IMeta
  (-meta [coll] meta)

  ISeqable
  (-seq [coll] coll)

  ISequential
  IEquiv
  (-equiv [coll other] (equiv-sequential coll other))

  ASeq
  ISeq
  (-first [coll]
    (aget node off))
  (-rest [coll]
    (if (< (inc off) (alength node))
      (let [s (chunked-seq vec node i (inc off))]
        (if (nil? s)
          ()
          s))
      (-chunked-rest coll)))

  INext
  (-next [coll]
    (if (< (inc off) (alength node))
      (let [s (chunked-seq vec node i (inc off))]
        (if (nil? s)
          nil
          s))
      (-chunked-next coll)))

  IDrop
  (-drop [coll n]
    (let [o (+ off n)]
      (if (< o (alength node))
        (chunked-seq vec node i o)
        (let [i (+ i o)]
          (if (< i (-count vec))
            (let [new-offset (js-mod i 32)]
              (chunked-seq vec (unchecked-array-for vec i) (- i new-offset) new-offset))
            nil)))))

  ICollection
  (-conj [coll o]
    (cons o coll))

  IEmptyableCollection
  (-empty [coll]
    ())

  ICounted
  (-count [coll]
    (- (-count vec) (+ i off)))

  IChunkedSeq
  (-chunked-first [coll]
    (array-chunk node off))
  (-chunked-rest [coll]
    (let [end (+ i (alength node))]
      (if (< end (-count vec))
        (chunked-seq vec (unchecked-array-for vec end) end 0)
        ())))

  IChunkedNext
  (-chunked-next [coll]
    (let [end (+ i (alength node))]
      (when (< end (-count vec))
        (chunked-seq vec (unchecked-array-for vec end) end 0))))

  IHash
  (-hash [coll] (caching-hash coll hash-ordered-coll __hash))

  IReduce
  (-reduce [coll f]
    (pv-reduce vec f (+ i off) (count vec)))

  (-reduce [coll f start]
    (pv-reduce vec f start (+ i off) (count vec))))
