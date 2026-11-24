(require '[clojure.edn :as edn])
(doseq [text ["" "not-a-uuid" "1-2-3-4" "1-2-3-4-5-6" "-1-2-3-4-5" "-1-2-3-4" "1--2-3-4" "1-2-3-4-" "1-2-3-4-g" "0x1-2-3-4-5" "12345678901234567-2-3-4-5" "8000000000000000-2-3-4-5" "0000000000000000000000000001-2-3-4-5" " 1-2-3-4-5" "1-2-3-4-5 " "1-2-3-4-+" "1-2-3-4-\ud800"]]
  (let [result (try [(str (java.util.UUID/fromString text))] (catch Throwable e [(ex-message e) (mapv int (ex-message e))]))]
    (println (pr-str (pr-str [text result])))))
