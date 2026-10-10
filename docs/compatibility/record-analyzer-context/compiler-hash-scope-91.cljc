(defn hash-scope [s]
  (hash-combine #?(:clj  (hash (:name s))
                   :cljs (-hash ^not-native (:name s)))
    (shadow-depth s)))
