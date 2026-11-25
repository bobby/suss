(assert (= "1.12.6" (clojure-version)))
(let [expected (read-string (slurp "primary.stdout")) actual (read-string (slurp "native.stdout"))]
  (assert (= 5 (count expected) (count actual)))
  (doseq [[index p n] (map vector (range) expected actual)] (prn {:index index :expected p :actual n :match (= p n)}))
  (spit "primary-rows.stdout" (apply str (map #(str (pr-str %) "\n") expected)))
  (when (not= expected actual) (System/exit 1)))
