(assert (= "1.12.6" (clojure-version)))
(let [rows (mapv (fn [units]
  (let [text (String. (char-array (map char units)))]
    (try (let [p (re-pattern text)]
      {:units units :valid true :pattern-units (mapv int (.pattern p)) :flags (.flags p)
       :str-units (mapv int (str p)) :printed-units (mapv int (pr-str p))})
      (catch Throwable e
        {:units units :valid false :exception (.getName (class e))
         :message-units (mapv int (ex-message e))
         :description-units (when (instance? java.util.regex.PatternSyntaxException e)
                              (mapv int (.getDescription e)))
         :index (when (instance? java.util.regex.PatternSyntaxException e) (.getIndex e))}))))
  (read-string (slurp (first *command-line-args*))))]
  (spit (second *command-line-args*) (pr-str rows))
  (println (pr-str {:cases (count rows) :accepted (count (filter :valid rows))
                   :rejected (count (remove :valid rows))})))
