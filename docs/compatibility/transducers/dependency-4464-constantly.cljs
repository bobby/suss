(defn constantly
  "Returns a function that takes any number of arguments and returns x."
  [x] (fn [& args] x))
