(deftype StringIter [s ^:mutable i]
  Object
  (hasNext [_] (< i (.-length s)))
  (next [_]
    (let [ret (.charAt s i)]
      (set! i (inc i))
      ret))
  (remove [_] (js/Error. "Unsupported operation")))
