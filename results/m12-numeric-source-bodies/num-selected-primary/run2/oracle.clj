(require '[clojure.tools.reader :as reader]
         '[clojure.tools.reader.reader-types :as readers]
         '[clojure.test :as t]
         '[suss-oracle.clojure-test-suite-jvm :as oracle])
(assert (= "1.12.6" (clojure-version)))
(assert (= :suss (reader/read-string {:read-cond :allow :features #{:suss}}
                  "#?(:clj :jvm :suss :suss :default :default)")))
(let [[source output selected-output] *command-line-args*
      eof (Object.)
      selected (atom [])]
  (reset! oracle/state {:assertions [] :tests [] :ordinals {} :reporter-errors [] :skips [] :loading "clojure.core-test.num"})
  (binding [*ns* (the-ns 'user) *file* source]
    (let [input (readers/indexing-push-back-reader (slurp source))]
      (loop []
        (let [form (reader/read {:read-cond :allow :features #{:suss} :eof eof} input)]
          (when-not (identical? form eof)
            (swap! selected conj form)
            (eval form)
            (recur))))))
  (assert (nil? (ns-resolve 'clojure.core-test.num 'Checker)))
  (swap! oracle/state assoc :loading nil)
  (let [summary (binding [t/report @#'oracle/report]
                  (t/run-tests 'clojure.core-test.num))
        {:keys [assertions tests skips reporter-errors]} @oracle/state]
    (spit selected-output (str (clojure.string/join "\n" (map pr-str @selected)) "\n"))
    (spit output (#'oracle/json {"schema" 2 "suite" "95d4a9112cfc5fe6629be3d14bb99080ea00af2f"
                 "upstream" (str "clojure-" (clojure-version))
                 "feature-selection" ["suss" "default"]
                 "namespaces" ["clojure.core-test.num"] "tests" tests "skips" skips
                 "assertions" assertions "reporter-errors" reporter-errors
                 "summary" {"test" (:test summary) "pass" (:pass summary) "fail" (:fail summary) "error" (:error summary)}}))
    (assert (zero? (+ (:fail summary) (:error summary))))))
