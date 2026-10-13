(deftype ES6SetEntriesIterator [^:mutable s]
  Object
  (next [_]
    (if-not (nil? s)
      (let [x (first s)]
        (set! s (next s))
        #js {:value #js [x x] :done false})
      #js {:value nil :done true})))
