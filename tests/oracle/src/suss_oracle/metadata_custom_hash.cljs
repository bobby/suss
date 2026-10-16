(ns suss-oracle.metadata-custom-hash
  (:require [cljs.analyzer :as ana]
            [cljs.env :as env]
            [cljs.js :as cljs]
            [cljs.tools.reader :as reader]))

;; Original development-only probe. Replace an actual protocol implementation,
;; delegate to its original body, and observe identity through the macroexpander.
(def probe-ns 'suss-oracle.custom-hash-target)
(def macro-lib 'suss-oracle.custom-hash-macros)

(defn -main []
  (when (nil? cljs.core/NS_CACHE) (set! cljs.core/NS_CACHE (atom {})))
  (let [state (cljs/empty-state)
        cache cljs.core/NS_CACHE
        old-cache @cache
        proto (.-prototype cljs.core/PersistentArrayMap)
        method "cljs$core$IHash$_hash$arity$1"
        original (aget proto method)
        subject (array nil)
        calls (array 0)
        seen (array)
        windows (array)
        macro-ns (symbol (str macro-lib "$macros"))
        analyze (fn [form]
                  (binding [env/*compiler* state ana/*cljs-ns* probe-ns
                            ana/*analyze-deps* false ana/*load-macros* false]
                    (ana/analyze (assoc (ana/empty-env)
                                  :ns (ana/get-namespace probe-ns)) form)))
        observer (fn [_ macro-env]
                   (let [stored (get-in macro-env [:ns :defs 'watched])
                         resolved (ana/resolve-var macro-env 'watched)
                         ast-info (:info (ana/analyze macro-env 'watched))]
                     (.push seen #js {:envMeta (:meta stored)
                                     :resolvedMeta (:meta resolved)
                                     :astMeta (:meta ast-info)
                                     :audit (:audit (:meta stored))})
                     17))
        invoke (fn []
                 (let [ast (analyze (reader/read-string
                                     "(suss-oracle.custom-hash-macros/observe)"))]
                   (assert (= :const (:op ast)))
                   (assert (= 17 (:val ast)))))
        window (fn [label f]
                 (let [before (aget calls 0) value (f)]
                   (.push windows #js {:stage label
                                       :subject-custom-hash-calls (- (aget calls 0) before)})
                   value))]
    (assert (fn? original))
    (swap! state assoc-in [::ana/namespaces probe-ns] {:name probe-ns :defs {}})
    (swap! state assoc-in [::ana/namespaces macro-ns]
      {:name macro-ns
       :defs {'observe {:name (symbol (str macro-ns) "observe")
                        :macro true :method-params [['&form '&env]] :max-fixed-arity 2}}})
    (set! (.-cljs$lang$macro observer) true)
    (swap! cache assoc macro-ns (create-ns macro-ns #js {:observe observer}))
    (try
      (analyze (reader/read-string "(def ^{:audit {:x 17}} watched 17)"))
      (let [saved (get-in @state [::ana/namespaces probe-ns :defs 'watched :meta])
            audit (:audit saved)
            _ (assert (instance? cljs.core/PersistentArrayMap audit))
            initial-x (:x audit)
            initial-memo (.-__hash audit)
            _ (aset subject 0 audit)
            _ (window "macro-original" invoke)
            replacement (fn [coll]
                          (this-as receiver
                            (when (identical? receiver (aget subject 0))
                              (aset calls 0 (inc (aget calls 0))))
                            (.call original receiver coll)))
            _ (aset proto method replacement)
            _ (window "macro-custom-before-warming" invoke)
            before-miss (.-__hash audit)
            first-hash (window "explicit-custom-hash-miss" #(hash audit))
            warmed-memo (.-__hash audit)
            _ (window "macro-custom-after-warming" invoke)
            before-hit (.-__hash audit)
            second-hash (window "explicit-custom-hash-hit" #(hash audit))
            first (aget seen 0)
            changed (aget seen 1)
            warmed (aget seen 2)]
        (assert (= 3 (.-length seen)))
        (assert (pos? (aget calls 0)))
        (println (.stringify js/JSON
          #js {:schema 1
               :boundary "Pinned CLJS macroexpander; delegated PersistentArrayMap IHash replacement; no Suss transport"
               :windows windows
               :custom-method-installed (identical? replacement (aget proto method))
               :memo #js {:initial-nil (nil? initial-memo)
                          :miss-was-nil-before-call (nil? before-miss)
                          :hit-was-numeric-before-call (number? before-hit)
                          :hit-kept-memo-identity (identical? before-hit (.-__hash audit))
                          :warmed-numeric (number? warmed-memo)
                          :miss-equals-memo (= first-hash warmed-memo)
                          :hit-equals-miss (= second-hash first-hash)}
               :identity #js {:original (identical? saved (.-envMeta first))
                              :custom-before-warming (identical? saved (.-envMeta changed))
                              :custom-after-warming (identical? saved (.-envMeta warmed))
                              :custom-resolved (identical? saved (.-resolvedMeta warmed))
                              :custom-ast (identical? saved (.-astMeta warmed))
                              :nested-before-warming (identical? audit (.-audit changed))
                              :nested-after-warming (identical? audit (.-audit warmed))}
               :values #js {:x-before initial-x :x-after (:x (.-audit warmed))}})))
      (finally
        (aset proto method original)
        (reset! cache old-cache)))))

(set! *main-cli-fn* -main)
