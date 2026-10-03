;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype PersistentHashSet [meta hash-map ^:mutable __hash]
  Object
  (toString [coll]
    (pr-str* coll))
  (equiv [this other]
    (-equiv this other))
  (keys [coll]
    (es6-iterator (seq coll)))
  (entries [coll]
    (es6-set-entries-iterator (seq coll)))
  (values [coll]
    (es6-iterator (seq coll)))
  (has [coll k]
    (contains? coll k))
  (forEach [coll f]
    (doseq [[k v] coll]
      (f v k)))

  ICloneable
  (-clone [_] (PersistentHashSet. meta hash-map __hash))

  IIterable
  (-iterator [coll]
    (HashSetIter. (-iterator hash-map)))

  IWithMeta
  (-with-meta [coll new-meta]
    (if (identical? new-meta meta)
      coll
      (PersistentHashSet. new-meta hash-map __hash)))

  IMeta
  (-meta [coll] meta)

  ICollection
  (-conj [coll o]
    (let [m (-assoc hash-map o nil)]
      (if (identical? m hash-map)
        coll
        (PersistentHashSet. meta m nil))))

  IEmptyableCollection
  (-empty [coll] (-with-meta (.-EMPTY PersistentHashSet) meta))

  IEquiv
  (-equiv [coll other]
    (and
     (set? other)
     (== (count coll) (count other))
     ^boolean
     (try
       (reduce-kv
         #(or (contains? other %2) (reduced false))
         true hash-map)
       (catch js/Error ex
         false))))

  IHash
  (-hash [coll] (caching-hash coll hash-unordered-coll __hash))

  ISeqable
  (-seq [coll] (keys hash-map))

  ICounted
  (-count [coll] (-count hash-map))

  ILookup
  (-lookup [coll v]
    (-lookup coll v nil))
  (-lookup [coll v not-found]
    (if-let [entry (-find hash-map v)]
      (key entry)
      not-found))

  ISet
  (-disjoin [coll v]
    (let [m (-dissoc hash-map v)]
      (if (identical? m hash-map)
        coll
        (PersistentHashSet. meta m nil))))

  IFn
  (-invoke [coll k]
    (-lookup coll k))
  (-invoke [coll k not-found]
    (-lookup coll k not-found))

  IEditableCollection
  (-as-transient [coll] (TransientHashSet. (-as-transient hash-map))))
