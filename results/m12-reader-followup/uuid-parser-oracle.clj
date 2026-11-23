(assert (= "1.12.6" (clojure-version)))
(let [rows (mapv (fn [units]
  (let [text (String. (char-array (map char units)))]
    (try (let [u (java.util.UUID/fromString text)]
      {:units units :bits [(.getMostSignificantBits u) (.getLeastSignificantBits u)]
       :canonical (str u) :hash (.hashCode u) :error-units nil})
      (catch Throwable e {:units units :bits nil :canonical nil :hash nil
                          :error-units (mapv int (ex-message e))}))))
(read-string (slurp (nth *command-line-args* 2))))]
  (spit (first *command-line-args*) (pr-str rows))
  (println (pr-str {:cases (count rows) :accepted (count (filter :bits rows)) :rejected (count (remove :bits rows))})))
(with-open [out (clojure.java.io/output-stream (second *command-line-args*))]
  (.write out (byte-array (map #(inc (Character/digit (char %) 16)) (range 65536)))))
