(defn- tree-map-append [left right]
  (cond
    (nil? left)
    right

    (nil? right)
    left

    (instance? RedNode left)
    (if (instance? RedNode right)
      (let [app (tree-map-append (.-right left) (.-left right))]
        (if (instance? RedNode app)
          (RedNode. (.-key app) (.-val app)
                    (RedNode. (.-key left) (.-val left)
                              (.-left left)
                              (.-left app)
                              nil)
                    (RedNode. (.-key right) (.-val right)
                              (.-right app)
                              (.-right right)
                              nil)
                    nil)
          (RedNode. (.-key left) (.-val left)
                    (.-left left)
                    (RedNode. (.-key right) (.-val right) app (.-right right) nil)
                    nil)))
      (RedNode. (.-key left) (.-val left)
                (.-left left)
                (tree-map-append (.-right left) right)
                nil))

    (instance? RedNode right)
    (RedNode. (.-key right) (.-val right)
              (tree-map-append left (.-left right))
              (.-right right)
              nil)

    :else
    (let [app (tree-map-append (.-right left) (.-left right))]
      (if (instance? RedNode app)
        (RedNode. (.-key app) (.-val app)
                  (BlackNode. (.-key left) (.-val left)
                              (.-left left)
                              (.-left app)
                              nil)
                  (BlackNode. (.-key right) (.-val right)
                              (.-right app)
                              (.-right right)
                              nil)
                  nil)
        (balance-left-del (.-key left) (.-val left)
                          (.-left left)
                          (BlackNode. (.-key right) (.-val right)
                                      app
                                      (.-right right)
                                      nil))))))
