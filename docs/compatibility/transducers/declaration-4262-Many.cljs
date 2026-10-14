(deftype Many [vals]
  Object
  (add [this o]
    (.push vals o)
    this)
  (remove [this]
    (.shift vals))
  (isEmpty [this]
    (zero? (.-length vals)))
  (toString [this]
    (str_ "Many: " vals)))
