(defn ^boolean instance?
  "Evaluates x and tests if it is an instance of the type
  c. Returns true or false"
  [c x]
  (cljs.core/instance? c x))
