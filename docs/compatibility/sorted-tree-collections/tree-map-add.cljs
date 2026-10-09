(defn- tree-map-add [comp tree k v found]
  (if (nil? tree)
    (RedNode. k v nil nil nil)
    (let [c (comp k (.-key tree))]
      (cond
        (zero? c)
        (do (aset found 0 tree)
            nil)

        (neg? c)
        (let [ins (tree-map-add comp (.-left tree) k v found)]
          (if-not (nil? ins)
            (.add-left tree ins)))

        :else
        (let [ins (tree-map-add comp (.-right tree) k v found)]
          (if-not (nil? ins)
            (.add-right tree ins)))))))
