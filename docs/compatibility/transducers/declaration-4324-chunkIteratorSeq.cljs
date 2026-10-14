(defn- chunkIteratorSeq [iter]
  (lazy-seq
    (when ^boolean (.hasNext iter)
      (let [arr (array)]
        (loop [n 0]
          (if (and (.hasNext iter) (< n 32))
            (do
              (aset arr n (.next iter))
              (recur (inc n)))
            (chunk-cons (array-chunk arr 0 n) (chunkIteratorSeq iter))))))))
