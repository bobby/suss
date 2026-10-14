(defn butlast
  "Return a seq of all but the last item in coll, in linear time"
  [s]
  (loop [ret [] s s]
    (if (next s)
      (recur (conj ret (first s)) (next s))
      (seq ret))))
