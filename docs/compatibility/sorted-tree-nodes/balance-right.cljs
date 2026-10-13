(defn- balance-right [key val left ins]
  (if (instance? RedNode ins)
    (cond
      (instance? RedNode (.-right ins))
      (RedNode. (.-key ins) (.-val ins)
                (BlackNode. key val left (.-left ins) nil)
                (.blacken (.-right ins))
                nil)

      (instance? RedNode (.-left ins))
      (RedNode. (.. ins -left -key) (.. ins -left -val)
                (BlackNode. key val left (.. ins -left -left) nil)
                (BlackNode. (.-key ins) (.-val ins)
                            (.. ins -left -right)
                            (.-right ins)
                            nil)
                nil)

      :else
      (BlackNode. key val left ins nil))
    (BlackNode. key val left ins nil)))
