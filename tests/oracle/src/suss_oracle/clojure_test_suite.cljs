(ns suss-oracle.clojure-test-suite
  "Development oracle for the vendored jank-lang/clojure-test-suite.

  Every cljs.test assertion report becomes one observation: the test var,
  its assertion ordinal, testing contexts, the original form, the oracle
  verdict and tagged operand values. Suss decides its own assertions from the
  same observations host-side, never from these verdicts alone."
  (:require [cljs.test :as t]
            [suss-oracle.clojure-test-suite-skips :as skips]
            [suss-oracle.clojure-test-suite-namespaces :as suite]))

;; Transport tags match suss-oracle.main/encode. That encoder throws on values
;; it cannot represent; this one is bounded and reports them as opaque so an
;; infinite sequence or a JavaScript object cannot abort the run. Observation
;; must not change the observed program: it never realizes a pending value, so
;; a sequence with an unrealized tail is opaque rather than forced.
(def node-budget 20000)

(defn f64-bits [value]
  (let [view (js/DataView. (js/ArrayBuffer. 8))]
    (.setFloat64 view 0 value false)
    (str (.padStart (.toString (.getUint32 view 0 false) 16) 8 "0")
         (.padStart (.toString (.getUint32 view 4 false) 16) 8 "0"))))

(defn- opaque [value reason]
  #js {:tag "opaque" :type (pr-str (type value)) :reason reason})

(defn- unrealized? [value]
  (and (implements? IPending value) (not (realized? value))))

(defn- realized-items
  "Items of a fully realized sequence, or ::unrealized without forcing anything."
  [budget value]
  (loop [tail value items []]
    (cond
      (unrealized? tail) ::unrealized
      :else (let [s (seq tail)]
              (if (nil? s)
                items
                (do (when (neg? (swap! budget dec))
                      (throw (ex-info "oracle encoding budget exhausted" {::budget true})))
                    (recur (rest s) (conj items (first s)))))))))

(defn- encode* [budget value]
  (when (neg? (swap! budget dec))
    (throw (ex-info "oracle encoding budget exhausted" {::budget true})))
  (let [encode (partial encode* budget)
        items (fn [coll] (into-array (map encode coll)))]
    (cond
      (instance? cljs.core/ExceptionInfo value)
      #js {:tag "exception-info" :data (encode (ex-data value))
           :message (encode (ex-message value)) :cause (encode (ex-cause value))}
      (nil? value) #js {:tag "nil"}
      (boolean? value) #js {:tag "bool" :value value}
      (number? value) #js {:tag "f64" :bits (f64-bits value)}
      (string? value) (do (when (neg? (swap! budget - (.-length value)))
                            (throw (ex-info "oracle encoding budget exhausted" {::budget true})))
                          #js {:tag "string" :units (into-array (map #(.charCodeAt value %) (range (.-length value))))})
      (keyword? value) #js {:tag "keyword" :namespace (encode (namespace value)) :name (encode (name value))}
      (symbol? value) #js {:tag "symbol" :namespace (encode (namespace value)) :name (encode (name value))}
      (map? value) #js {:tag "map" :entries (into-array (map (fn [[k v]] #js [(encode k) (encode v)]) value))}
      (set? value) #js {:tag "set" :items (items value)}
      (vector? value) #js {:tag "vector" :items (items value)}
      (unrealized? value) (opaque value "unrealized")
      (seq? value) (let [realized (realized-items budget value)]
                     (if (= ::unrealized realized)
                       (opaque value "unrealized")
                       #js {:tag "seq" :items (items realized)}))
      :else (opaque value "unsupported"))))

(defn encode [value]
  (try
    (encode* (atom node-budget) value)
    (catch :default error
      (if (::budget (ex-data error))
        (opaque value "budget")
        (opaque value (str "encode-error: " (ex-message error)))))))

(def state (atom nil))

(defn- current-test []
  (let [v (first (:testing-vars (t/get-current-env)))]
    (when v (str (:ns (meta v)) "/" (:name (meta v))))))

(defn- predicate-operands
  "cljs.test/assert-predicate reports `(pred values...)` on pass and
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
                      "thrown" #js {:threw (= :pass (:type m))}
                      "predicate" #js {:operands (into-array (map encode operands))}
                      "value" #js {:value (encode actual)}
                      "error" #js {:thrown (encode actual) :message (encode (ex-message actual))})]
    (swap! state update :assertions conj
           (js/Object.assign
            #js {:test test :ordinal ordinal :kind kind
                 :verdict (name (:type m))
                 :form (pr-str form)
                 :line (:line m) :column (:column m)
                 :contexts (into-array (reverse (:testing-contexts (t/get-current-env))))}
            observation))))

(defn- guarded-record! [m]
  (try
    (record! m)
    (catch :default error
      (swap! state update :reporter-errors conj
             #js {:test (current-test) :type (name (:type m))
                  :error (str error) :stack (.-stack error)}))))

(defmethod t/report [::observe :pass] [m] (t/inc-report-counter! :pass) (guarded-record! m))
(defmethod t/report [::observe :fail] [m] (t/inc-report-counter! :fail) (guarded-record! m))
(defmethod t/report [::observe :error] [m] (t/inc-report-counter! :error) (guarded-record! m))
(defmethod t/report [::observe :begin-test-var] [m]
  (when (.-SUSS_CTS_TRACE (.-env js/process)) (.error js/console "begin" (str (:var m))))
  (swap! state update :tests conj (str (:ns (meta (:var m))) "/" (:name (meta (:var m))))))
(declare write-output!)
(defmethod t/report [::observe :end-run-tests] [m]
  ;; Some suite tests are asynchronous, so the run ends here, not when
  ;; run-all-tests returns.
  (swap! state assoc :summary (select-keys m [:test :pass :fail :error]))
  (write-output!))
(defmethod t/report [::observe :default] [_])

(defn- write-output! []
  (let [{:keys [output assertions tests summary reporter-errors]} @state
        fs (js/require "fs")]
    (.writeFileSync fs output
      (.stringify js/JSON
        #js {:schema 1
             :suite suite/commit
             :upstream "c4295f303100bbf5afac449242d30bca1126f1a1"
             :namespaces (into-array suite/namespaces)
             :tests (into-array tests)
             :skips (into-array (skips/observed))
             :assertions (into-array assertions)
             :reporter-errors (into-array reporter-errors)
             :summary (clj->js summary)}))))

(defn- seed-random!
  "Replace Math.random with mulberry32 so each seeded run is reproducible.
  Comparing runs with different seeds identifies nondeterministic assertions."
  [seed]
  (let [state (atom (js/parseInt seed 10))]
    (set! (.-random js/Math)
          (fn []
            (let [a (swap! state #(bit-or (+ % 0x6D2B79F5) 0))
                  t (js/Math.imul (bit-xor a (unsigned-bit-shift-right a 15)) (bit-or a 1))
                  t (bit-xor t (+ t (js/Math.imul (bit-xor t (unsigned-bit-shift-right t 7)) (bit-or t 61))))]
              (/ (unsigned-bit-shift-right (bit-xor t (unsigned-bit-shift-right t 14)) 0) 4294967296))))))

(defn -main [output seed]
  (seed-random! seed)
  (reset! state {:output output :assertions [] :tests [] :ordinals {} :reporter-errors []})
  (skips/capture! (fn [] (current-test)))
  (t/run-all-tests #"clojure\..*-test\..*" (t/empty-env ::observe)))

(set! *main-cli-fn* -main)
