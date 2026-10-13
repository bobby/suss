(deftype ES6EntriesIterator [^:mutable s]
  Object
  (next [_]
    (if-not (nil? s)
      (let [[k v] (first s)]
        (set! s (next s))
        #js {:value #js [k v] :done false})
      #js {:value nil :done true})))
