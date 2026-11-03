;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype Repeat [meta count val ^:mutable next ^:mutable __hash]
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
    (-lastIndexOf coll x count))
  (lastIndexOf [coll x start]
    (-lastIndexOf coll x start))

  IPending
  (-realized? [coll] false)

  IWithMeta
  (-with-meta [coll new-meta]
    (if (identical? new-meta meta)
      coll
      (Repeat. new-meta count val next nil)))

  IMeta
  (-meta [coll] meta)

  ISeq
  (-first [coll]
    val)
  (-rest [coll]
    (if (nil? next)
      (if (> count 1)
        (do
          (set! next (Repeat. nil (dec count) val nil nil))
          next)
        (if (== -1 count)
          coll
          ()))
      next))

  INext
  (-next [coll]
    (if (nil? next)
      (if (> count 1)
        (do
          (set! next (Repeat. nil (dec count) val nil nil))
          next)
        (if (== -1 count)
          coll
          nil))
      next))

  ICollection
  (-conj [coll o] (cons o coll))

  IDrop
  (-drop [coll n]
    (if (== count -1)
      coll
      (let [dropped-count (- count n)]
        (when (pos? dropped-count)
          (Repeat. nil dropped-count val nil nil)))))

  IEmptyableCollection
  (-empty [coll] (.-EMPTY List))

  IHash
  (-hash [coll] (caching-hash coll hash-ordered-coll __hash))

  ISequential
  ISeqable
  (-seq [coll] coll)

  IEquiv
  (-equiv [coll other] (equiv-sequential coll other))

  IReduce
  (-reduce [coll f]
    (if (== count -1)
      (loop [ret (f val val)]
        (if (reduced? ret)
          @ret
          (recur (f ret val))))
      (loop [i 1 ret val]
        (if (< i count)
          (let [ret (f ret val)]
            (if (reduced? ret)
              @ret
              (recur (inc i) ret)))
          ret))))
  (-reduce [coll f start]
    (if (== count -1)
      (loop [ret (f start val)]
        (if (reduced? ret)
          @ret
          (recur (f ret val))))
      (loop [i 0 ret start]
        (if (< i count)
          (let [ret (f ret val)]
            (if (reduced? ret)
              @ret
              (recur (inc i) ret)))
          ret)))))
