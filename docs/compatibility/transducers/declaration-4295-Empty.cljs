(deftype Empty []
  Object
  (add [this o]
    (Single. o))
  (remove [this]
    (throw (js/Error. (str_ "Removing object from empty buffer"))))
  (isEmpty [this]
    true)
  (toString [this]
    "Empty"))
