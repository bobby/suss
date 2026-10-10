(defn ^:dynamic find-ns-starts-with [needle]
  (reduce-kv
    (fn [xs ns _]
      (when (= needle (get-first-ns-segment ns))
        (reduced needle)))
    nil
    (::ana/namespaces @env/*compiler*)))
