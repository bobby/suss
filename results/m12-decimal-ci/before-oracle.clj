
(prn (pr-str 1M))

(prn (pr-str 1.50M))

(prn (pr-str -0.00M))

(prn (pr-str [1.50M -0.00M 123456789012345678901234567890.00M]))

(prn (let [x (read-string "1.50M")] [(pr-str x) (decimal? x) (rational? x) (number? x) (= (class x) BigDecimal)]))

(prn (let [x 1.50M] [(str (class x)) (pr-str (class x)) (instance? BigDecimal x) (instance? Number x) (ifn? (class x))]))

(prn (let [c (class 1M) trace (atom 0)] [(try (c (do (swap! trace inc) 7)) false (catch Throwable _ true)) @trace]))

(prn ((fn [x] [(pr-str x) (decimal? x) (= (type x) BigDecimal)]) 1.25M))
(def ^:const decimal-ci-constant 1.50M) (def decimal-ci-read (fn [] decimal-ci-constant)) (def decimal-ci-constant 2.50M)
(prn [(pr-str (decimal-ci-read)) (pr-str decimal-ci-constant)])
(defprotocol DecimalCiProtocol (decimal-ci-kind [x])) (extend-protocol DecimalCiProtocol java.math.BigDecimal (decimal-ci-kind [x] :decimal) java.lang.Number (decimal-ci-kind [x] :number))
(prn [(decimal-ci-kind 1M) (decimal-ci-kind 1) (decimal-ci-kind 1N) (decimal-ci-kind 1/3)])
