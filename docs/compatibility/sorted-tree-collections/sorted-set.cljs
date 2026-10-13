(defn sorted-set
  "Returns a new sorted set with supplied keys."
  ([& keys]
   (reduce -conj (.-EMPTY PersistentTreeSet) keys)))
