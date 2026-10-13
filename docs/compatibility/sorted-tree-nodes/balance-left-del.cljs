(defn- balance-left-del [key val del right]
  (cond
    (instance? RedNode del)
    (RedNode. key val (.blacken del) right nil)

    (instance? BlackNode right)
    (balance-right key val del (.redden right))

    (and (instance? RedNode right) (instance? BlackNode (.-left right)))
    (RedNode. (.. right -left -key) (.. right -left -val)
              (BlackNode. key val del (.. right -left -left) nil)
              (balance-right (.-key right) (.-val right)
                             (.. right -left -right)
                             (.redden (.-right right)))
              nil)

    :else
    (throw (js/Error. "red-black tree invariant violation"))))
