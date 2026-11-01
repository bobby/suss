;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype PersistentTreeMap [comp tree cnt meta ^:mutable __hash]
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

  (entry-at [coll k]
    (loop [t tree]
      (if-not (nil? t)
        (let [c (comp k (.-key t))]
          (cond (zero? c) t
                (neg? c)  (recur (.-left t))
                :else     (recur (.-right t)))))))

  ICloneable
  (-clone [_] (PersistentTreeMap. comp tree cnt meta __hash))

  IWithMeta
  (-with-meta [coll new-meta]
    (if (identical? new-meta meta)
      coll
      (PersistentTreeMap. comp tree cnt new-meta __hash)))

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
  (-empty [coll] (PersistentTreeMap. comp nil 0 meta 0))

  IEquiv
  (-equiv [coll other] (equiv-map coll other))

  IHash
  (-hash [coll] (caching-hash coll hash-unordered-coll __hash))

  ICounted
  (-count [coll] cnt)

  IKVReduce
  (-kv-reduce [coll f init]
    (if-not (nil? tree)
      (unreduced (tree-map-kv-reduce tree f init))
      init))

  IFn
  (-invoke [coll k]
    (-lookup coll k))

  (-invoke [coll k not-found]
    (-lookup coll k not-found))

  ISeqable
  (-seq [coll]
    (if (pos? cnt)
      (create-tree-map-seq tree true cnt)))

  IReversible
  (-rseq [coll]
    (if (pos? cnt)
      (create-tree-map-seq tree false cnt)))

  ILookup
  (-lookup [coll k]
    (-lookup coll k nil))

  (-lookup [coll k not-found]
    (let [n (.entry-at coll k)]
      (if-not (nil? n)
        (.-val n)
        not-found)))

  IAssociative
  (-assoc [coll k v]
    (let [found (array nil)
          t     (tree-map-add comp tree k v found)]
      (if (nil? t)
        (let [found-node (nth found 0)]
          (if (= v (.-val found-node))
            coll
            (PersistentTreeMap. comp (tree-map-replace comp tree k v) cnt meta nil)))
        (PersistentTreeMap. comp (.blacken t) (inc cnt) meta nil))))

  (-contains-key? [coll k]
    (not (nil? (.entry-at coll k))))

  IFind
  (-find [coll k]
    (.entry-at coll k))

  IMap
  (-dissoc [coll k]
    (let [found (array nil)
          t     (tree-map-remove comp tree k found)]
      (if (nil? t)
        (if (nil? (nth found 0))
          coll
          (PersistentTreeMap. comp nil 0 meta nil))
        (PersistentTreeMap. comp (.blacken t) (dec cnt) meta nil))))

  ISorted
  (-sorted-seq [coll ascending?]
    (if (pos? cnt)
      (create-tree-map-seq tree ascending? cnt)))

  (-sorted-seq-from [coll k ascending?]
    (if (pos? cnt)
      (loop [stack nil t tree]
        (if-not (nil? t)
          (let [c (comp k (.-key t))]
            (cond
              (zero? c)  (PersistentTreeMapSeq. nil (conj stack t) ascending? -1 nil)
              ascending? (if (neg? c)
                           (recur (conj stack t) (.-left t))
                           (recur stack          (.-right t)))
              :else      (if (pos? c)
                           (recur (conj stack t) (.-right t))
                           (recur stack          (.-left t)))))
          (when-not (nil? stack)
            (PersistentTreeMapSeq. nil stack ascending? -1 nil))))))

  (-entry-key [coll entry] (key entry))

  (-comparator [coll] comp))
