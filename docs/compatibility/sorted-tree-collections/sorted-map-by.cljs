(defn sorted-map-by
  "keyval => key val
  Returns a new sorted map with supplied mappings, using the supplied comparator."
  ([comparator & keyvals]
     (loop [in (seq keyvals)
            out (PersistentTreeMap. (fn->comparator comparator) nil 0 nil 0)]
       (if in
         (recur (nnext in) (assoc out (first in) (second in)))
         out))))
