(defn not-empty
  "If coll is empty, returns nil, else coll"
  [coll] (when (seq coll) coll))
