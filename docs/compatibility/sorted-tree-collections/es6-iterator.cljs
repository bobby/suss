(defn es6-iterator
  "Return a ES2015+ compatible iterator for coll."
  [coll]
  (ES6Iterator. (seq coll)))
