(defn ^string munge-str
  "Munge string `name` without considering `..` or JavaScript reserved keywords."
  [name]
  (let [sb (StringBuffer.)]
    (loop [i 0]
      (if (< i (. name -length))
        (let [c (.charAt name i)
              sub (gobject/get CHAR_MAP c)]
          (if-not (nil? sub)
            (.append sb sub)
            (.append sb c))
          (recur (inc i)))))
    (.toString sb)))
