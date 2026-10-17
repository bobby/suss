(ns suss-oracle.metadata-memo-leaf
  (:require [cljs.analyzer :as ana] [cljs.env :as env]
            [cljs.js :as cljs] [cljs.tools.reader :as reader]))

;; Authored development-only observations on the pinned self-hosted analyzer.
;; They establish reference behavior, not Suss write authorization.
(def identity-fields
  ["env-meta" "resolved-meta" "ast-meta" "env-leaf" "resolved-leaf" "ast-leaf"])

(defn run-case [kind warm? nil-aware?]
  (when (nil? cljs.core/NS_CACHE) (set! cljs.core/NS_CACHE (atom {})))
  (let [state (cljs/empty-state) cache cljs.core/NS_CACHE old-cache @cache
        target 'suss-oracle.memo-leaf-target
        macro-lib 'suss-oracle.memo-leaf-macros
        macro-ns (symbol (str macro-lib "$macros"))
        ctor (if (= kind :symbol) cljs.core/Symbol cljs.core/Keyword)
        proto (.-prototype ctor)
        method "cljs$core$IHash$_hash$arity$1" original (aget proto method)
        subject (array nil) saved-meta (array nil) action (array nil)
        stage (array "baseline") observations (array) dispatches (array 0)
        analyze (fn [form]
                  (binding [env/*compiler* state ana/*cljs-ns* target
                            ana/*analyze-deps* false ana/*load-macros* false]
                    (ana/analyze (assoc (ana/empty-env) :ns (ana/get-namespace target)) form)))
        observer (fn [_ macro-env]
                   (when-let [f (aget action 0)] (f))
                   (let [stored (get-in macro-env [:ns :defs 'watched])
                         resolved (ana/resolve-var macro-env 'watched)
                         info (:info (ana/analyze macro-env 'watched))
                         m (aget saved-meta 0) leaf (aget subject 0)]
                     (.push observations
                       #js {:stage (aget stage 0)
                            :env-meta (identical? m (:meta stored))
                            :resolved-meta (identical? m (:meta resolved))
                            :ast-meta (identical? m (:meta info))
                            :env-leaf (identical? leaf (:audit-leaf (:meta stored)))
                            :resolved-leaf (identical? leaf (:audit-leaf (:meta resolved)))
                            :ast-leaf (identical? leaf (:audit-leaf (:meta info)))
                            :memo (.-_hash leaf) :leaf-text (str leaf)
                            :dispatches (aget dispatches 0)})
                     17))
        invoke (fn []
                 (let [ast (analyze (reader/read-string
                                     "(suss-oracle.memo-leaf-macros/observe)"))]
                   (assert (= :const (:op ast)))
                   (assert (= 17 (:val ast)))))]
    (assert (fn? original))
    (try
      (swap! state assoc-in [::ana/namespaces target] {:name target :defs {}})
      (swap! state assoc-in [::ana/namespaces macro-ns]
        {:name macro-ns :defs {'observe {:name (symbol (str macro-ns) "observe")
                                       :macro true :method-params [['&form '&env]]
                                       :max-fixed-arity 2}}})
      (set! (.-cljs$lang$macro observer) true)
      (swap! cache assoc macro-ns (create-ns macro-ns #js {:observe observer}))
      (let [leaf-name (str "memo-" (name kind) "-leaf-" (if warm? "warm" "plain")
                           (when nil-aware? "-nil-aware"))
            source (str "(def ^{:audit-leaf " (when (= kind :keyword) ":")
                        leaf-name "} watched 17)")
            ast (analyze (reader/read-string source))
            m (get-in @state [::ana/namespaces target :defs 'watched :meta])
            leaf (:audit-leaf m) initial-memo (.-_hash leaf)
            leaf-text (str leaf)]
        (assert (= 17 (:val (:init ast))))
        (assert (if (= kind :symbol) (symbol? leaf) (keyword? leaf)))
        (aset subject 0 leaf)
        (aset saved-meta 0 m)
        (let [canonical (when warm? (hash leaf))
              before-custom-memo (.-_hash leaf)]
          (when warm?
            (assert (number? canonical))
            (assert (= canonical before-custom-memo)))
          (invoke)
          (aset proto method
            (fn [coll]
              ;; Pinned hash's fast path passes null as the formal argument;
              ;; the physical protocol receiver is JavaScript this.
              (this-as receiver
                (if (identical? receiver (aget subject 0))
                  (do (aset dispatches 0 (inc (aget dispatches 0)))
                      (let [memo (.-_hash receiver)]
                        (if (and nil-aware? (some? memo)) memo
                          (do (set! (.-_hash receiver) 91) 91))))
                  (.call original receiver coll)))))
          (aset stage 0 "custom-write")
          (aset action 0 (fn []
                           (assert (= (if (and nil-aware? warm?) canonical 91)
                                      (hash leaf)))))
          (invoke)
          (aset action 0 nil)
          (aset proto method original)
          (aset stage 0 "restored-method")
          (invoke)
          #js {:schema 2 :nil-aware nil-aware? :kind (name kind) :explicit-warm warm?
               :source source :declaration-value (:val (:init ast))
               :initial-memo initial-memo :initial-memo-type (js* "typeof ~{}" initial-memo)
               :canonical-hash canonical :before-custom-memo before-custom-memo
               :before-custom-memo-type (js* "typeof ~{}" before-custom-memo)
               :leaf-text leaf-text :observations observations
               :dispatches (aget dispatches 0)}))
      (finally
        (aset proto method original)
        (reset! cache old-cache)))))

(defn -main []
  (doseq [nil-aware? [false true] kind [:symbol :keyword] warm? [false true]]
    (let [result (run-case kind warm? nil-aware?)
          rows (array-seq (.-observations result))]
      ;; Preserve the actual row before checking it; unknown errors are uncaught.
      (js/console.log (.stringify js/JSON result))
      (assert (= 3 (count rows)))
      (assert (= 1 (.-dispatches result)))
      (assert (every? (fn [row]
                        (every? (fn [key] (true? (aget row key))) identity-fields)) rows))
      (assert (= ["baseline" "custom-write" "restored-method"]
                 (mapv #(aget % "stage") rows)))
      (let [expected (if (and nil-aware? warm?) (aget result "canonical-hash") 91)]
        (assert (= [expected expected] (mapv #(aget % "memo") (rest rows)))))
      (assert (every? #(= (aget result "leaf-text") (aget % "leaf-text")) rows)))))

(set! *main-cli-fn* -main)
