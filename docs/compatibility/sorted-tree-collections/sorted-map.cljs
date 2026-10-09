(defn sorted-map
  "keyval => key val
  Returns a new sorted map with supplied mappings."
  ([& keyvals]
     (loop [in (seq keyvals) out (.-EMPTY PersistentTreeMap)]
       (if in
         (recur (nnext in) (assoc out (first in) (second in)))
         out))))
