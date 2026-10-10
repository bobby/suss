(defn nil-iter []
  (reify
    Object
    (hasNext [_] false)
    (next [_] (js/Error. "No such element"))
    (remove [_] (js/Error. "Unsupported operation"))))
