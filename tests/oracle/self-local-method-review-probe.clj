;; Original development-only review probe; no JVM is shipped.
(require '[cljs.analyzer :as ana] '[cljs.env :as env] 'cljs.core)
(def observations (atom []))
(defmacro inspect-self [label parameter]
  (let [argument (get-in &env [:locals parameter])
        self (if (= parameter 'self) (:shadow argument) (get-in &env [:locals 'self]))
        first-staged (:method-params self)
        staged-self (-> first-staged first first :shadow)
        duplicate-shadow (-> first-staged second second :shadow)
        row [label (:arg-id argument) (:tag argument)
             (get-in argument [:shadow :local]) (get-in argument [:shadow :arg-id])
             (get-in argument [:shadow :fn-var])
             (get-in argument [:env :context])
             (:fn-var self) (:variadic? self) (:max-fixed-arity self)
             (:local staged-self) (contains? staged-self :fn-var)
             (:arg-id duplicate-shadow)]]
    (swap! observations conj row)
    parameter))
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'review]
    (ana/analyze (assoc (ana/empty-env) :ns {:name 'review :defs {} :use-macros {'inspect-self 'user}})
      (read-string "(def f (fn self ([self] (inspect-self :first self)) ([x x & rest] (inspect-self :second x))))"))))
(def expected '[[:first 0 nil :fn nil true :expr true true 2 :fn false 0]
                [:second 1 nil :arg 0 nil :expr true true 2 :fn false 0]])
(assert (= @observations expected) (pr-str @observations))
(prn @observations)
