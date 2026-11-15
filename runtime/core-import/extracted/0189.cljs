;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype PersistentArrayMapSeq [arr i _meta]
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

  IMeta
  (-meta [coll] _meta)

  IWithMeta
  (-with-meta [coll new-meta]
    (if (identical? new-meta _meta)
      coll
      (PersistentArrayMapSeq. arr i new-meta)))

  ICounted
  (-count [coll]
    (/ (- (alength arr) i) 2))

  ISeqable
  (-seq [coll] coll)

  ISequential
  IEquiv
  (-equiv [coll other] (equiv-sequential coll other))

  ICollection
  (-conj [coll o]
    (cons o coll))

  IEmptyableCollection
  (-empty [coll] (.-EMPTY List))

  IHash
  (-hash [coll] (hash-ordered-coll coll))

  ISeq
  (-first [coll]
    (MapEntry. (aget arr i) (aget arr (inc i)) nil))

  (-rest [coll]
    (if (< i (- (alength arr) 2))
      (PersistentArrayMapSeq. arr (+ i 2) nil)
      ()))

  INext
  (-next [coll]
    (when (< i (- (alength arr) 2))
      (PersistentArrayMapSeq. arr (+ i 2) nil)))

  IDrop
  (-drop [coll n]
    (when (< n (-count coll))
      (PersistentArrayMapSeq. arr (+ i (* 2 n)) nil)))

  IReduce
  (-reduce [coll f] (seq-reduce f coll))
  (-reduce [coll f start] (seq-reduce f start coll)))
