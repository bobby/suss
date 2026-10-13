(defn- tree-map-remove [comp tree k found]
  (if-not (nil? tree)
    (let [c (comp k (.-key tree))]
      (cond
        (zero? c)
        (do (aset found 0 tree)
            (tree-map-append (.-left tree) (.-right tree)))

        (neg? c)
        (let [del (tree-map-remove comp (.-left tree) k found)]
          (if (or (not (nil? del)) (not (nil? (aget found 0))))
            (if (instance? BlackNode (.-left tree))
              (balance-left-del (.-key tree) (.-val tree) del (.-right tree))
              (RedNode. (.-key tree) (.-val tree) del (.-right tree) nil))))

        :else
        (let [del (tree-map-remove comp (.-right tree) k found)]
          (if (or (not (nil? del)) (not (nil? (aget found 0))))
            (if (instance? BlackNode (.-right tree))
              (balance-right-del (.-key tree) (.-val tree) (.-left tree) del)
              (RedNode. (.-key tree) (.-val tree) (.-left tree) del nil))))))))
