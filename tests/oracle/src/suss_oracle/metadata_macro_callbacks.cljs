(ns suss-oracle.metadata-macro-callbacks
  (:require [cljs.analyzer :as ana]
            [cljs.env :as env]
            [cljs.js :as cljs]
            [cljs.tools.reader :as reader]))

;; Original development-only probe. The macro is a compiled CLJS function
;; installed through bootstrap Namespace/Var APIs; the pinned macroexpander
;; performs resolution, arity checking and invocation. No Suss transport runs.
(def probe-ns 'suss-oracle.callback-target)
(def macro-lib 'suss-oracle.callback-macros)

(defn -main []
  (when (nil? cljs.core/NS_CACHE) (set! cljs.core/NS_CACHE (atom {})))
  (let [state (cljs/empty-state)
        cache cljs.core/NS_CACHE
        old-cache @cache
        original-hash cljs.core/hash
        trace (array)
        total (array 0)
        windows (array)
        seen (array)
        macro-ns (symbol (str macro-lib "$macros"))
        analyze (fn [form]
                  (binding [env/*compiler* state
                            ana/*cljs-ns* probe-ns
                            ana/*analyze-deps* false
                            ana/*load-macros* false]
                    (ana/analyze (assoc (ana/empty-env)
                                  :ns (ana/get-namespace probe-ns)) form)))
        window (fn [label f]
                 (set! (.-length trace) 0)
                 (aset total 0 0)
                 (let [value (f)]
                   (.push windows #js {:stage label
                                       :selected-numeric-hash-calls (.slice trace)
                                       :total-hash-calls (aget total 0)})
                   value))
        observer (fn [_ macro-env]
                   (let [stored (get-in macro-env [:ns :defs 'watched])
                         resolved (ana/resolve-var macro-env 'watched)
                         ast-info (:info (ana/analyze macro-env 'watched))]
                     (assert (every? map? [stored resolved ast-info]))
                     (assert (= (symbol (str probe-ns) "watched") (:name ast-info)))
                     (.push seen #js {:envMeta (:meta stored)
                                     :resolvedMeta (:meta resolved)
                                     :astMeta (:meta ast-info)
                                     :astInfo ast-info})
                     17))
        invoke (fn []
                 (let [ast (analyze (reader/read-string
                                     "(suss-oracle.callback-macros/observe)"))]
                   (assert (= :const (:op ast)))
                   (assert (= 17 (:val ast)))))
        metadata (fn [] (get-in @state [::ana/namespaces probe-ns :defs 'watched :meta]))]
    (swap! state assoc-in [::ana/namespaces probe-ns] {:name probe-ns :defs {}})
    (swap! state assoc-in [::ana/namespaces macro-ns]
      {:name macro-ns
       :defs {'observe {:name (symbol (str macro-ns) "observe")
                        :macro true :method-params [['&form '&env]] :max-fixed-arity 2}}})
    (set! (.-cljs$lang$macro observer) true)
    (swap! cache assoc macro-ns (create-ns macro-ns #js {:observe observer}))
    (try
      (set! cljs.core/hash
        (fn [value]
          (aset total 0 (inc (aget total 0)))
          (when (and (number? value) (<= 101 value 109)) (.push trace value))
          (original-hash value)))
      (let [form (window "read-declaration"
                   #(reader/read-string
                      "(def ^{:audit #{101 102 103 104 105 106 107 108 109}} watched 17)"))
            _ (window "analyze-pre-read-declaration" #(analyze form))
            saved (metadata)
            _ (window "macro-first" invoke)
            _ (window "macro-second" invoke)
            _ (window "explicit-hash-warming" #(hash (:audit saved)))
            _ (window "macro-after-warming" invoke)
            occurrence (window "read-fresh-set-occurrence"
                         #(reader/read-string "#{101 102 103 104 105 106 107 108 109}"))
            _ (window "analyze-pre-read-set-occurrence" #(analyze occurrence))
            changed (window "read-new-revision"
                      #(reader/read-string "(def ^{:audit #{101 109}} watched 18)"))
            _ (window "analyze-new-revision" #(analyze changed))
            _ (window "macro-new-revision" invoke)
            current (metadata)
            first (aget seen 0)
            second (aget seen 1)
            warmed (aget seen 2)
            newer (aget seen 3)]
        (assert (= 4 (.-length seen)))
        (assert (= [101 102 103 104 105 106 107 108 109] (vec (sort (:audit saved)))))
        (assert (= [101 109] (vec (sort (:audit current)))))
        (println (.stringify js/JSON
          #js {:schema 1
               :boundary "Pinned CLJS macroexpander with a bootstrap-registered compiled CLJS macro; no Suss transport"
               :windows windows
               :macro-calls (.-length seen)
               :identity #js {:first-env-meta (identical? saved (.-envMeta first))
                              :second-env-meta (identical? saved (.-envMeta second))
                              :resolved-meta (identical? saved (.-resolvedMeta second))
                              :ast-meta (identical? saved (.-astMeta second))
                              :ast-info-fresh (not (identical? (.-astInfo first) (.-astInfo second)))
                              :after-warming (identical? saved (.-astMeta warmed))
                              :new-revision (not (identical? saved (.-astMeta newer)))
                              :new-current-meta (identical? current (.-astMeta newer))}
               :values #js {:old-audit (into-array (sort (:audit saved)))
                            :new-audit (into-array (sort (:audit current)))}})))
      (finally
        (set! cljs.core/hash original-hash)
        (reset! cache old-cache)))))

(set! *main-cli-fn* -main)
