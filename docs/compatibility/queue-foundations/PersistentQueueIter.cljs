(deftype PersistentQueueIter [^:mutable fseq riter]
  Object
  (hasNext [_]
    (or (and (some? fseq) (seq fseq)) (and (some? riter) (.hasNext riter))))
  (next [_]
    (cond
      (some? fseq)
      (let [ret (first fseq)]
        (set! fseq (next fseq))
        ret)
      (and (some? riter) ^boolean (.hasNext riter))
      (.next riter)
      :else (throw (js/Error. "No such element"))))
  (remove [_] (js/Error. "Unsupported operation")))