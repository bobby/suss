(defn mk-bound-fn
  [sc test key]
  (fn [e]
    (let [comp (-comparator sc)]
      (test (comp (-entry-key sc e) key) 0))))
