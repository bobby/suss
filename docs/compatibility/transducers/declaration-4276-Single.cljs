(deftype Single [^:mutable val]
  Object
  (add [this o]
    (if (identical? val NONE)
      (do
        (set! val o)
        this)
      (Many. #js [val o])))
  (remove [this]
    (if (identical? val NONE)
      (throw (js/Error. (str_ "Removing object from empty buffer")))
      (let [ret val]
        (set! val NONE)
        ret)))
  (isEmpty [this]
    (identical? val NONE))
  (toString [this]
    (str_ "Single: " val)))
