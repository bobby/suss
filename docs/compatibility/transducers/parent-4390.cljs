(set! (.-createMulti TransformerIterator)
  (fn [xform sources]
    (transformer-iterator xform (MultiIterator. (to-array sources)) true)))
