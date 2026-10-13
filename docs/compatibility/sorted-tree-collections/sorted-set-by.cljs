(defn sorted-set-by
  "Returns a new sorted set with supplied keys, using the supplied comparator."
  ([comparator & keys]
   (reduce -conj
           (PersistentTreeSet. nil (sorted-map-by comparator) 0)
           keys)))
