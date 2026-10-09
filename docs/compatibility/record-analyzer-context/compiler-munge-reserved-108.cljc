(defn munge-reserved [reserved]
  (fn [s]
    (if-not (nil? (get reserved s))
      (str s "$")
      s)))
