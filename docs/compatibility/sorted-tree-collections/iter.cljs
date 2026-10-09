(defn iter [coll]
  (cond
    (iterable? coll) (-iterator coll)
    (nil? coll) (nil-iter)
    (string? coll) (string-iter coll)
    (array? coll) (array-iter coll)
    (seqable? coll) (seq-iter coll)
    :else (throw (js/Error. (str_ "Cannot create iterator from " coll)))))
