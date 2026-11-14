;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype PersistentArrayMap [meta cnt arr ^:mutable __hash]
  Object
  (toString [coll]
    (pr-str* coll))
  (equiv [this other]
    (-equiv this other))
  (keys [coll]
    (es6-iterator (keys coll)))
  (entries [coll]
    (es6-entries-iterator (seq coll)))
  (values [coll]
    (es6-iterator (vals coll)))
  (has [coll k]
    (contains? coll k))
  (get [coll k not-found]
    (-lookup coll k not-found))
  (forEach [coll f]
    (doseq [[k v] coll]
      (f v k)))

  ICloneable
  (-clone [_] (PersistentArrayMap. meta cnt arr __hash))

  IWithMeta
  (-with-meta [coll new-meta]
    (if (identical? new-meta meta)
      coll
      (PersistentArrayMap. new-meta cnt arr __hash)))

  IMeta
  (-meta [coll] meta)

  ICollection
  (-conj [coll entry]
    (if (vector? entry)
      (-assoc coll (-nth entry 0) (-nth entry 1))
      (loop [ret coll es (seq entry)]
        (if (nil? es)
          ret
          (let [e (first es)]
            (if (vector? e)
              (recur (-assoc ret (-nth e 0) (-nth e 1))
                     (next es))
              (throw (js/Error. "conj on a map takes map entries or seqables of map entries"))))))))

  IEmptyableCollection
  (-empty [coll] (-with-meta (.-EMPTY PersistentArrayMap) meta))

  IEquiv
  (-equiv [coll other]
    (if (and (map? other) (not (record? other)))
      (let [alen (alength arr)
            ^not-native other other]
        (if (== cnt (-count other))
          (loop [i 0]
            (if (< i alen)
              (let [v (-lookup other (aget arr i) lookup-sentinel)]
                (if-not (identical? v lookup-sentinel)
                  (if (= (aget arr (inc i)) v)
                    (recur (+ i 2))
                    false)
                  false))
              true))
          false))
      false))

  IHash
  (-hash [coll] (caching-hash coll hash-unordered-coll __hash))

  IIterable
  (-iterator [this]
    (PersistentArrayMapIterator. arr 0 (* cnt 2)))

  ISeqable
  (-seq [coll]
    (persistent-array-map-seq arr 0 nil))

  IDrop
  (-drop [coll n]
    (when-some [s (-seq coll)]
      (-drop s n)))

  ICounted
  (-count [coll] cnt)

  ILookup
  (-lookup [coll k]
    (-lookup coll k nil))

  (-lookup [coll k not-found]
    (let [idx (array-map-index-of coll k)]
      (if (== idx -1)
        not-found
        (aget arr (inc idx)))))

  IAssociative
  (-assoc [coll k v]
    (let [idx (array-map-index-of coll k)]
      (cond
        (== idx -1)
        (if (< cnt (.-HASHMAP-THRESHOLD PersistentArrayMap))
          (let [arr (array-map-extend-kv coll k v)]
            (PersistentArrayMap. meta (inc cnt) arr nil))
          (-> (into (.-EMPTY PersistentHashMap) coll)
            (-assoc k v)
            (-with-meta meta)))

        (identical? v (aget arr (inc idx)))
        coll

        :else
        (let [arr (doto (aclone arr)
                    (aset (inc idx) v))]
          (PersistentArrayMap. meta cnt arr nil)))))

  (-contains-key? [coll k]
    (not (== (array-map-index-of coll k) -1)))

  IFind
  (-find [coll k]
    (let [idx (array-map-index-of coll k)]
      (when-not (== idx -1)
        (MapEntry. (aget arr idx) (aget arr (inc idx)) nil))))

  IMap
  (-dissoc [coll k]
    (let [idx (array-map-index-of coll k)]
      (if (>= idx 0)
        (let [len     (alength arr)
              new-len (- len 2)]
          (if (zero? new-len)
            (-empty coll)
            (let [new-arr (make-array new-len)]
              (loop [s 0 d 0]
                (cond
                  (>= s len) (PersistentArrayMap. meta (dec cnt) new-arr nil)
                  (= k (aget arr s)) (recur (+ s 2) d)
                  :else (do (aset new-arr d (aget arr s))
                            (aset new-arr (inc d) (aget arr (inc s)))
                            (recur (+ s 2) (+ d 2))))))))
        coll)))

  IKVReduce
  (-kv-reduce [coll f init]
    (let [len (alength arr)]
      (loop [i 0 init init]
        (if (< i len)
          (let [init (f init (aget arr i) (aget arr (inc i)))]
            (if (reduced? init)
              @init
              (recur (+ i 2) init)))
          init))))

  IReduce
  (-reduce [coll f]
    (iter-reduce coll f))
  (-reduce [coll f start]
    (iter-reduce coll f start))

  IFn
  (-invoke [coll k]
    (-lookup coll k))

  (-invoke [coll k not-found]
    (-lookup coll k not-found))

  IEditableCollection
  (-as-transient [coll]
    (TransientArrayMap. (js-obj) (alength arr) (aclone arr))))
