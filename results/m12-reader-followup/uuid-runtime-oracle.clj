(require 'clojure.edn)
(assert (= "1.12.6" (clojure-version)))
(defn quote-units [text]
  (str "\"" (apply str (map #(format "\\u%04x" (int %)) text)) "\""))
(let [rows (read-string (slurp (first *command-line-args*)))
      results
      (mapv (fn [units]
        (let [text (String. (char-array (map char units)))
              tagged (str "#uuid " (quote-units text))
              expression (str "(try (let [u (suss.edn/read-string " (pr-str tagged)
                              ")] [(str u) (hash u)]) (catch :default e [:error (ex-message e)]))")
              result (try (let [u (clojure.edn/read-string tagged)] [(str u) (hash u)])
                          (catch Throwable e [:error (ex-message e)]))]
          (str "{\"expression\":" (quote-units expression) ",\"expected_units\":"
               "[" (clojure.string/join "," (map int (pr-str result))) "]}"))) rows)]
  (spit (second *command-line-args*) (str "[" (clojure.string/join ",\n" results) "]\n"))
  (println (count rows)))
