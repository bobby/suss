(set! (.-create TransformerIterator)
  (fn [xform source]
    (transformer-iterator xform source false)))
