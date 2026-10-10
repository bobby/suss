(defn shadow-depth [s]
  (let [{:keys [name info]} s]
    (loop [d 0, {:keys [shadow]} info]
      (cond
        shadow (recur (inc d) shadow)
        (find-ns-starts-with (str name)) (inc d)
        :else d))))
