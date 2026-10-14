(deftype MultiIterator [iters]
  Object
  (hasNext [_]
    (loop [iters (seq iters)]
      (if-not (nil? iters)
        (let [iter (first iters)]
          (if-not ^boolean (.hasNext iter)
            false
            (recur (next iters))))
        true)))
  (next [_]
    (let [nexts (array)]
      (dotimes [i (alength iters)]
        (aset nexts i (.next (aget iters i))))
      (prim-seq nexts 0))))
