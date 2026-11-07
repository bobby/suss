(ns suss-oracle.clojure-test-suite-jvm
  "Development oracle for the vendored jank-lang/clojure-test-suite on JVM
  Clojure (ADR-0003, #334).

  Every clojure.test assertion report becomes one observation: the test var,
  its assertion ordinal, testing contexts, the original form, the oracle
  verdict and tagged operand values, in the records the ClojureScript runner
  wrote before it. Suss decides its own assertions from the same observations
  host-side, never from these verdicts alone. Development only: Suss never
  ships or links Java."
  (:require [clojure.string :as str]
            [clojure.test :as t]))

;; Transport tags extend suss-oracle.main's with Clojure's numeric types.
;; A character is a one-unit string, as Suss represents characters. Encoding
;; is bounded and reports what it can't represent as opaque, and it never
;; realizes a pending value, so observation can't change the program.
(def node-budget 20000)

(defn- spend! [budget n]
  (when (neg? (swap! budget - n))
    (throw (ex-info "oracle encoding budget exhausted" {::budget true}))))

(defn- opaque [value reason]
  {"tag" "opaque" "type" (pr-str (type value)) "reason" reason})

(defn- unrealized? [value]
  (and (instance? clojure.lang.IPending value) (not (realized? value))))

(defn- realized-items
  "Items of a fully realized sequence, or ::unrealized without forcing anything."
  [budget value]
  (loop [tail value items []]
    (if (unrealized? tail)
      ::unrealized
      (let [s (seq tail)]
        (if (nil? s)
          items
          (do (spend! budget 1)
              (recur (rest s) (conj items (first s)))))))))

(defn- units [^String s]
  (mapv int s))

(defn- encode* [budget value]
  (spend! budget 1)
  (let [encode (partial encode* budget)
        items (fn [coll] (mapv encode coll))]
    (cond
      (instance? clojure.lang.ExceptionInfo value)
      {"tag" "exception-info" "data" (encode (ex-data value))
       "message" (encode (ex-message value)) "cause" (encode (ex-cause value))}
      (nil? value) {"tag" "nil"}
      (boolean? value) {"tag" "bool" "value" value}
      (or (instance? Double value) (instance? Float value))
      {"tag" "f64" "bits" (format "%016x" (Double/doubleToRawLongBits (double value)))}
      (or (instance? Long value) (instance? Integer value) (instance? Short value) (instance? Byte value))
      {"tag" "long" "value" (str value)}
      (or (instance? clojure.lang.BigInt value) (instance? java.math.BigInteger value))
      {"tag" "bigint" "value" (str (biginteger value))}
      (ratio? value)
      {"tag" "ratio" "numerator" (str (numerator value)) "denominator" (str (denominator value))}
      (decimal? value)
      {"tag" "decimal" "unscaled" (str (.unscaledValue ^BigDecimal value)) "scale" (.scale ^BigDecimal value)}
      (char? value) {"tag" "string" "units" [(int value)]}
      (string? value) (do (spend! budget (count value))
                          {"tag" "string" "units" (units value)})
      (keyword? value) {"tag" "keyword" "namespace" (encode (namespace value)) "name" (encode (name value))}
      (symbol? value) {"tag" "symbol" "namespace" (encode (namespace value)) "name" (encode (name value))}
      (map? value) {"tag" "map" "entries" (mapv (fn [[k v]] [(encode k) (encode v)]) value)}
      (set? value) {"tag" "set" "items" (items value)}
      (vector? value) {"tag" "vector" "items" (items value)}
      (unrealized? value) (opaque value "unrealized")
      (seq? value) (let [realized (realized-items budget value)]
                     (if (= ::unrealized realized)
                       (opaque value "unrealized")
                       {"tag" "seq" "items" (items realized)}))
      :else (opaque value "unsupported"))))

(defn encode [value]
  (try
    (encode* (atom node-budget) value)
    (catch Throwable error
      (if (::budget (ex-data error))
        (opaque value "budget")
        (opaque value (str "encode-error: " (ex-message error)))))))

;; A small JSON writer, so the oracle needs nothing beyond Clojure itself.
(defn- json-string [^String s]
  (let [out (StringBuilder. "\"")]
    (doseq [c s]
      (case c
        \" (.append out "\\\"")
        \\ (.append out "\\\\")
        \newline (.append out "\\n")
        \return (.append out "\\r")
        \tab (.append out "\\t")
        (if (< (int c) 32)
          (.append out (format "\\u%04x" (int c)))
          (.append out c))))
    (str (.append out "\""))))

(defn- json [value]
  (cond
    (nil? value) "null"
    (boolean? value) (str value)
    (string? value) (json-string value)
    (integer? value) (str value)
    (map? value) (str "{" (str/join "," (map (fn [[k v]] (str (json-string k) ":" (json v))) value)) "}")
    (sequential? value) (str "[" (str/join "," (map json value)) "]")
    :else (throw (ex-info "unsupported JSON value" {:value value}))))

(def state (atom nil))

(defn- current-test []
  (when-let [v (first t/*testing-vars*)]
    (str (:ns (meta v)) "/" (:name (meta v)))))

(defn- predicate-operands
  "clojure.test's assert-predicate reports `(pred values...)` on pass and
  `(not (pred values...))` on failure, with `pred` as the quoted head symbol."
  [m form]
  (let [actual (:actual m)
        head (when (seq? form) (first form))
        call (case (:type m)
               :pass actual
               :fail (when (and (seq? actual) (= 'not (first actual))) (second actual))
               nil)]
    (when (and (symbol? head) (seq? call) (= head (first call)))
      (vec (rest call)))))

(defn- record! [m]
  (let [test (current-test)
        ordinal (get-in (swap! state update-in [:ordinals test] (fnil inc 0)) [:ordinals test])
        form (:expected m)
        thrown? (and (seq? form) (= 'p/thrown? (first form)))
        operands (when-not (or thrown? (= :error (:type m))) (predicate-operands m form))
        kind (cond (= :error (:type m)) "error"
                   thrown? "thrown"
                   operands "predicate"
                   :else "value")
        actual (:actual m)
        observation (case kind
                      "thrown" {"threw" (= :pass (:type m))}
                      "predicate" {"operands" (mapv encode operands)}
                      "value" {"value" (encode actual)}
                      "error" {"thrown" (encode actual)
                               "message" (encode (when (instance? Throwable actual) (ex-message actual)))})]
    (swap! state update :assertions conj
           (merge {"test" test "ordinal" ordinal "kind" kind
                   "verdict" (name (:type m))
                   "form" (binding [*print-namespace-maps* false] (pr-str form))
                   "line" (:line m) "column" (:column m)
                   "contexts" (vec (reverse t/*testing-contexts*))}
                  observation))))

(defn- guarded-record! [m]
  (try
    (record! m)
    (catch Throwable error
      (swap! state update :reporter-errors conj
             {"test" (current-test) "type" (name (:type m)) "error" (str error)}))))

(defn- report [m]
  (case (:type m)
    (:pass :fail :error) (do (t/inc-report-counter (:type m)) (guarded-record! m))
    :begin-test-var (let [v (:var m)]
                      (swap! state update :tests conj (str (:ns (meta v)) "/" (:name (meta v)))))
    nil))

;; when-var-exists prints `SKIP - sym` for a var the platform lacks. Each
;; line is attributed to the running test, or to the namespace being loaded.
(defn- skip-capturing-writer [^java.io.Writer delegate]
  (let [line (StringBuilder.)]
    (proxy [java.io.Writer] []
      (write
        ([x]
         (if (integer? x)
           (.write this (str (char x)))
           (doseq [c (str x)]
             (if (= c \newline)
               (let [text (str line)]
                 (.setLength line 0)
                 (if-let [[_ symbol] (re-matches #"SKIP - (\S+)\s*" text)]
                   (let [test (current-test)]
                     (swap! state update :skips conj
                            {"symbol" symbol
                             "phase" (if test "run" "load")
                             "namespace" (or (some-> test (str/split #"/") first) (:loading @state))
                             "test" test}))
                   (.write delegate (str text "\n"))))
               (.append line c)))))
        ([buffer offset length]
         (.write this (String. ^chars buffer (int offset) (int length)))))
      (flush [] (.flush delegate))
      (close [] (.close delegate)))))

(defn -main [output commit & namespaces]
  (reset! state {:assertions [] :tests [] :ordinals {} :reporter-errors [] :skips [] :loading nil})
  (let [writer (skip-capturing-writer *out*)
        summary (binding [*out* writer]
                  (doseq [name namespaces]
                    (swap! state assoc :loading name)
                    (require (symbol name)))
                  (swap! state assoc :loading nil)
                  (binding [t/report report]
                    (apply t/run-tests (map symbol namespaces))))
        {:keys [assertions tests skips reporter-errors]} @state]
    (spit output
          (json {"schema" 2
                 "suite" commit
                 "upstream" (str "clojure-" (clojure-version))
                 "namespaces" (vec namespaces)
                 "tests" tests
                 "skips" skips
                 "assertions" assertions
                 "reporter-errors" reporter-errors
                 "summary" {"test" (:test summary) "pass" (:pass summary)
                            "fail" (:fail summary) "error" (:error summary)}}))
    (shutdown-agents)))
