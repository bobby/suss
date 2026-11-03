;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype Subvec [meta v start end ^:mutable __hash]
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
  (-clone [_] (Subvec. meta v start end __hash))

  IWithMeta
  (-with-meta [coll new-meta]
    (if (identical? new-meta meta)
      coll
      (build-subvec new-meta v start end __hash)))

  IMeta
  (-meta [coll] meta)

  IStack
  (-peek [coll]
    (when-not (== start end)
      (-nth v (dec end))))
  (-pop [coll]
    (if (== start end)
      (throw (js/Error. "Can't pop empty vector"))
      (build-subvec meta v start (dec end) nil)))

  ICollection
  (-conj [coll o]
    (build-subvec meta (-assoc-n v end o) start (inc end) nil))

  IEmptyableCollection
  (-empty [coll] (-with-meta (.-EMPTY PersistentVector) meta))

  ISequential
  IEquiv
  (-equiv [coll other] (equiv-sequential coll other))

  IHash
  (-hash [coll] (caching-hash coll hash-ordered-coll __hash))

  ISeqable
  (-seq [coll]
    (let [subvec-seq (fn subvec-seq [i]
                       (when-not (== i end)
                         (cons (-nth v i)
                               (lazy-seq
                                (subvec-seq (inc i))))))]
      (subvec-seq start)))

  IReversible
  (-rseq [coll]
    (if-not (== start end)
      (RSeq. coll (dec (- end start)) nil)))

  ICounted
  (-count [coll] (- end start))

  IIndexed
  (-nth [coll n]
    (if (or (neg? n) (<= end (+ start n)))
      (vector-index-out-of-bounds n (- end start))
      (-nth v (+ start n))))
  (-nth [coll n not-found]
    (if (or (neg? n) (<= end (+ start n)))
      not-found
      (-nth v (+ start n) not-found)))

  ILookup
  (-lookup [coll k] (-lookup coll k nil))
  (-lookup [coll k not-found] (if (number? k)
                                (-nth coll k not-found)
                                not-found))

  IAssociative
  (-assoc [coll key val]
    (if (number? key)
      (-assoc-n coll key val)
      (throw (js/Error. "Subvec's key for assoc must be a number."))))
  (-contains-key? [coll key]
    (if (integer? key)
      (and (<= 0 key) (< key (- end start)))
      false))

  IFind
  (-find [coll n]
    (when-not (neg? n)
      (let [idx (+ start n)]
        (when (< idx end)
          (MapEntry. n (-lookup v idx) nil)))))

  IVector
  (-assoc-n [coll n val]
    (let [v-pos (+ start n)]
      (if (or (neg? n) (<= (inc end) v-pos))
        (throw (js/Error. (str_ "Index " n " out of bounds [0," (-count coll) "]")))
        (build-subvec meta (assoc v v-pos val) start (unchecked-max end (inc v-pos)) nil))))

  IReduce
  (-reduce [coll f]
    (if (implements? APersistentVector v)
      (pv-reduce v f start end)
      (ci-reduce coll f)))
  (-reduce [coll f init]
    (if (implements? APersistentVector v)
      (pv-reduce v f init start end)
      (ci-reduce coll f init)))

  IKVReduce
  (-kv-reduce [coll f init]
    (loop [i start j 0 init init]
      (if (< i end)
        (let [init (f init j (-nth v i))]
          (if (reduced? init)
            @init
            (recur (inc i) (inc j) init)))
        init)))

  IFn
  (-invoke [coll k]
    (-nth coll k))
  (-invoke [coll k not-found]
    (-nth coll k not-found))

  IIterable
  (-iterator [coll]
    (if (implements? APersistentVector v)
      (ranged-iterator v start end)
      (seq-iter coll))))
