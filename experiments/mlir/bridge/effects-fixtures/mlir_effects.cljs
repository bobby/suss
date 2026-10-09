;; Original development oracle; repository MIT/Apache-2.0 terms.
;; Compiled against ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1.
(ns suss-oracle.mlir-effects)
(def journal 0)
(def condition-count 0)
(defn mark! [digit] (set! journal (+ (* journal 10) digit)))
(defn bits [n] (let [buffer (.alloc js/Buffer 8)] (.writeDoubleBE buffer n) (.toString buffer "hex")))
(defn snapshot [value] #js {:result_bits (bits value) :journal_bits (bits journal) :count_bits (bits condition-count)})

(defn success []
  (set! journal 0)
  (set! condition-count 0)
  (let [body (fn []
               (let [condition (fn [] (mark! 1)
                                 (set! condition-count (+ condition-count 1))
                                 true)
                     decision (identical? (condition) true)
                     value (if decision (do (mark! 2) 7)
                                        (do (mark! 7) 8))]
                 value))
        handler (fn [payload] (mark! 3) payload)
        cleanup (fn [] (mark! 4) 0)
        inner (fn [] (try (body)
                         (catch :default payload (handler payload))
                         (finally (cleanup))))]
    (snapshot (inner))))

(defn alternative []
  (set! journal 0)
  (set! condition-count 0)
  (let [body (fn []
               (let [condition (fn [] (mark! 1)
                                 (set! condition-count (+ condition-count 1))
                                 false)
                     decision (identical? (condition) true)
                     value (if decision (do (mark! 2) 7)
                                        (do (mark! 7) 8))]
                 value))
        handler (fn [payload] (mark! 3) payload)
        cleanup (fn [] (mark! 4) 0)
        inner (fn [] (try (body)
                         (catch :default payload (handler payload))
                         (finally (cleanup))))]
    (snapshot (inner))))

(defn handled-throw []
  (set! journal 0)
  (set! condition-count 0)
  (let [body (fn []
               (let [condition (fn [] (mark! 1)
                                 (set! condition-count (+ condition-count 1))
                                 true)
                     decision (identical? (condition) true)
                     value (if decision (do (mark! 2) 7)
                                        (do (mark! 7) 8))]
                 (throw 17)))
        handler (fn [payload] (mark! 3) payload)
        cleanup (fn [] (mark! 4) 0)
        inner (fn [] (try (body)
                         (catch :default payload (handler payload))
                         (finally (cleanup))))]
    (snapshot (inner))))

(defn cleanup-overrides-success []
  (set! journal 0)
  (set! condition-count 0)
  (let [body (fn []
               (let [condition (fn [] (mark! 1)
                                 (set! condition-count (+ condition-count 1))
                                 true)
                     decision (identical? (condition) true)
                     value (if decision (do (mark! 2) 7)
                                        (do (mark! 7) 8))]
                 value))
        handler (fn [payload] (mark! 3) payload)
        cleanup (fn [] (mark! 4) (throw 99))
        inner (fn [] (try (body)
                         (catch :default payload (handler payload))
                         (finally (cleanup))))]
    (snapshot (try (inner) (catch :default payload (mark! 5) payload) (finally (mark! 6) 0)))))

(defn cleanup-overrides-handler []
  (set! journal 0)
  (set! condition-count 0)
  (let [body (fn []
               (let [condition (fn [] (mark! 1)
                                 (set! condition-count (+ condition-count 1))
                                 true)
                     decision (identical? (condition) true)
                     value (if decision (do (mark! 2) 7)
                                        (do (mark! 7) 8))]
                 (throw 17)))
        handler (fn [payload] (mark! 3) payload)
        cleanup (fn [] (mark! 4) (throw 99))
        inner (fn [] (try (body)
                         (catch :default payload (handler payload))
                         (finally (cleanup))))]
    (snapshot (try (inner) (catch :default payload (mark! 5) payload) (finally (mark! 6) 0)))))

(defn captured-f64 []
  (set! journal 0)
  (set! condition-count 0)
  (let [body (fn []
               (let [condition (fn [] (mark! 1)
                                 (set! condition-count (+ condition-count 1))
                                 true)
                     decision (identical? (condition) true)
                     value (if decision (do (mark! 2) (let [captured 7 callee (fn [] captured)] (callee)))
                                        (do (mark! 7) 8))]
                 value))
        handler (fn [payload] (mark! 3) payload)
        cleanup (fn [] (mark! 4) 0)
        inner (fn [] (try (body)
                         (catch :default payload (handler payload))
                         (finally (cleanup))))]
    (snapshot (inner))))

(defn nested-join []
  (set! journal 0)
  (set! condition-count 0)
  (let [body (fn []
               (let [condition (fn [] (mark! 1)
                                 (set! condition-count (+ condition-count 1))
                                 true)
                     decision (identical? (condition) true)
                     value (if decision (do (mark! 2) (if decision 7 7))
                                        (do (mark! 7) 8))]
                 value))
        handler (fn [payload] (mark! 3) payload)
        cleanup (fn [] (mark! 4) 0)
        inner (fn [] (try (body)
                         (catch :default payload (handler payload))
                         (finally (cleanup))))]
    (snapshot (inner))))

(defn -main []
  (println (.stringify js/JSON
    #js {:upstream "c4295f303100bbf5afac449242d30bca1126f1a1"
         :node (.-version js/process)
         :cases #js {:success (success)
                    :alternative (alternative)
                    :handled-throw (handled-throw)
                    :cleanup-overrides-success (cleanup-overrides-success)
                    :cleanup-overrides-handler (cleanup-overrides-handler)
                    :captured-f64 (captured-f64)
                    :nested-join (nested-join)}})))
(set! *main-cli-fn* -main)
