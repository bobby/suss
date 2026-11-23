(doseq [[label input] [["short" "`#uuid \"1-2-3-4-5\""]
                              ["signed" "`#uuid \"80000000-0000-0001-ffff-fffffffffffe\""]
                              ["positive" "`#uuid \"7fffffff-ffff-fffe-8000-000000000001\""]]]
  (let [source (str "(read-string " (pr-str input) ")")
        form (read-string input)
        value (second form)]
    (println (str "[" (pr-str label) "," (pr-str source) ","
                  (pr-str (pr-str form)) "," (pr-str (str (first form))) ","
                  (pr-str (str (.getMostSignificantBits ^java.util.UUID value))) ","
                  (pr-str (str (.getLeastSignificantBits ^java.util.UUID value))) "]"))))
