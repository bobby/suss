(defn- balance-right-del [key val left del]
  (cond
    (instance? RedNode del)
    (RedNode. key val left (.blacken del) nil)

    (instance? BlackNode left)
    (balance-left key val (.redden left) del)

    (and (instance? RedNode left) (instance? BlackNode (.-right left)))
    (RedNode. (.. left -right -key) (.. left -right -val)
              (balance-left (.-key left) (.-val left)
                            (.redden (.-left left))
                            (.. left -right -left))
              (BlackNode. key val (.. left -right -right) del nil)
              nil)

    :else
    (throw (js/Error. "red-black tree invariant violation"))))
