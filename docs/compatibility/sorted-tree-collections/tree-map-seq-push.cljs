(defn- tree-map-seq-push [node stack ^boolean ascending?]
  (loop [t node stack stack]
    (if-not (nil? t)
      (recur (if ascending? (.-left t) (.-right t))
             (conj stack t))
      stack)))
