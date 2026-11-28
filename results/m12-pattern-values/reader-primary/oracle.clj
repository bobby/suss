(assert (= "1.12.6" (clojure-version)))
(let [rows (mapv (fn [units]
  (try
    (let [value (read-string {:read-cond :allow :features #{:suss}} (String. (char-array (map char units))))]
      (if (instance? java.util.regex.Pattern value)
        {:units units :valid true :kind :pattern :pattern-units (mapv int (.pattern value)) :flags (.flags value) :printed-units (mapv int (pr-str value))}
        {:units units :valid true :kind :data :printed (pr-str value)}))
    (catch Throwable error {:units units :valid false :message (ex-message error)})))
  (read-string (slurp (first *command-line-args*))))]
  (spit (second *command-line-args*) (pr-str rows))
  (println (pr-str {:cases (count rows) :accepted (count (filter :valid rows)) :rejected (count (remove :valid rows))})))
