(defn- tree-map-replace [comp tree k v]
  (let [tk (.-key tree)
        c  (comp k tk)]
    (cond (zero? c) (.replace tree tk v (.-left tree) (.-right tree))
          (neg? c)  (.replace tree tk (.-val tree) (tree-map-replace comp (.-left tree) k v) (.-right tree))
          :else     (.replace tree tk (.-val tree) (.-left tree) (tree-map-replace comp (.-right tree) k v)))))
