(deftype PersistentQueueSeq [meta front rear ^:mutable __hash]
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
      (PersistentQueueSeq. new-meta front rear __hash)))

  IMeta
  (-meta [coll] meta)

  ISeq
  (-first [coll] (first front))
  (-rest  [coll]
    (if-let [f1 (next front)]
      (PersistentQueueSeq. meta f1 rear nil)
      (if (nil? rear)
        (-empty coll)
        (PersistentQueueSeq. meta rear nil nil))))

  INext
  (-next [coll]
    (if-let [f1 (next front)]
      (PersistentQueueSeq. meta f1 rear nil)
      (when (some? rear)
        (PersistentQueueSeq. meta rear nil nil))))

  ICollection
  (-conj [coll o] (cons o coll))

  IEmptyableCollection
  (-empty [coll] (-with-meta (.-EMPTY List) meta))

  ISequential
  IEquiv
  (-equiv [coll other] (equiv-sequential coll other))

  IHash
  (-hash [coll] (caching-hash coll hash-ordered-coll __hash))

  ISeqable
  (-seq [coll] coll))