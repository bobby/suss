(defn eduction
  "Returns a reducible/iterable application of the transducers
  to the items in coll. Transducers are applied in order as if
  combined with comp. Note that these applications will be
  performed every time reduce/iterator is called."
  {:arglists '([xform* coll])}
  [& xforms]
  (Eduction. (apply comp (butlast xforms)) (last xforms)))
