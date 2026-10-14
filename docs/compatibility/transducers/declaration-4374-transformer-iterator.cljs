(defn transformer-iterator
  [xform sourceIter multi]
  (let [iterator (TransformerIterator. EMPTY NONE false nil sourceIter multi)]
    (set! (.-xf iterator)
      (xform (fn
               ([] nil)
               ([acc] acc)
               ([acc o]
                (set! (.-buffer iterator) (.add (.-buffer iterator) o))
                acc))))
    iterator))
