(assert (= "1.12.6" (clojure-version)))
(prn {:clojure-version (clojure-version)})
(doseq [s ["0" "9007199254740993" "9223372036854775807" "-9223372036854775808" "9223372036854775808" "bad" "+17" "١٧" "１７" " 17" "" "-0"]]
  (prn {:input s
        :parse-long (try [:ok (parse-long s)] (catch Exception e [:error (.getName (class e))]))
        :java-long (try [:ok (Long/parseLong s)] (catch Exception e [:error (.getName (class e))]))}))
