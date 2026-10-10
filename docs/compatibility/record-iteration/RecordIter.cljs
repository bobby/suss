(deftype RecordIter [^:mutable i record base-count fields ext-map-iter]
  Object
  (hasNext [_]
    (or (< i base-count) (.hasNext ext-map-iter)))
  (next [_]
    (if (< i base-count)
      (let [k (nth fields i)]
        (set! i (inc i))
        (MapEntry. k (-lookup record k) nil))
      (.next ext-map-iter)))
  (remove [_] (js/Error. "Unsupported operation")))
