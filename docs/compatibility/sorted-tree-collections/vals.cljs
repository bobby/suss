(defn vals
  "Returns a sequence of the map's values, in the same order as (seq map)."
  [map]
  (when-let [mseq (seq map)]
    (ValSeq. mseq nil)))
