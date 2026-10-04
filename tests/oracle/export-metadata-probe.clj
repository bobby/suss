;; Original development-only observations against pinned ClojureScript.
;; No analyzer implementation is copied; Java remains a test oracle only.
(require '[cljs.analyzer :as ana] '[cljs.env :as env] 'cljs.core)
(def observations (atom []))
(defmacro inspect-export [name]
  (let [project (fn [defs]
                  (let [d (get defs name)]
                    [(contains? defs name) (:export d) (get-in d [:meta :export])]))]
    (swap! observations conj [name (project (get-in &env [:ns :defs]))
                             (project (:defs (ana/get-namespace 'export-probe)))])
    nil))
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'export-probe]
    (swap! env/*compiler* assoc-in [::ana/namespaces 'export-probe]
      {:name 'export-probe :defs {} :use-macros {'inspect-export 'user}})
    (doseq [source ["(def ^:export direct (inspect-export direct))"
                    "(inspect-export direct)"
                    "(def ^:export staged (let [] (inspect-export staged)))"
                    "(inspect-export staged)"
                    "(def ^{:export false} ^:export disabled 1)"
                    "(inspect-export disabled)"
                    "(def ^{:export nil} absent 1)"
                    "(inspect-export absent)"
                    "(def ^{:export \"renamed\"} renamed 1)"
                    "(inspect-export renamed)"]]
      (ana/analyze (assoc (ana/empty-env) :ns (ana/get-namespace 'export-probe))
                   (read-string source)))))
(assert (= @observations
  '[[direct [false nil nil] [true true nil]]
    [direct [true true true] [true true true]]
    [staged [false nil nil] [true true nil]]
    [staged [true true true] [true true true]]
    [disabled [true false false] [true false false]]
    [absent [true nil nil] [true nil nil]]
    [renamed [true "renamed" "renamed"] [true "renamed" "renamed"]]])
  (pr-str @observations))
(doseq [row @observations] (prn row))
