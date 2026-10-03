; Original development-only probe against the pinned analyzer.
; Run from tests/oracle: clojure -Srepro -M declaration-metadata-review-probe.clj
(require '[cljs.analyzer :as ana] '[cljs.env :as env] 'cljs.core)
(defmacro pr160-provisional []
  (let [actual (select-keys (get-in @env/*compiler* [::ana/namespaces 'review :defs 'pending]) [:name :meta])]
    (assert (= actual '{:name review/pending :meta [:raw]}) (pr-str actual))
    (prn :provisional actual))
  0)
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'review ana/*cljs-file* "review.cljs"]
    (doseq [s ["(def ^:dynamic f (fn [] 7))" "(def ^{:tag false} g (fn [] 7))" "(def ^{:tag false} scalar 7)" "(def ^{:name custom :test true} raw 7)" "(def ^{:top-fn {:name replaced :private false :doc \"override\" :meta {:test :replacement} :ret-tag wrong}} overlay (fn [] 7))"]]
      (let [f (read-string s) e (assoc (ana/empty-env) :ns {:name 'review :defs {}})]
        (ana/analyze e f)
        (let [actual (select-keys (get-in @env/*compiler* [::ana/namespaces 'review :defs (second f)]) [:name :tag :ret-tag :private :doc :meta])
              expected (get '{f {:name review/f :ret-tag number :meta {:dynamic true :file nil}}
                              g {:name review/g :tag false :ret-tag number :meta {:tag false :file nil}}
                              scalar {:name review/scalar :tag number :meta {:tag false :file nil}}
                              raw {:name custom :tag number :meta {:name custom :file nil}}
                              overlay {:name replaced :ret-tag number :private false :doc "override" :meta {:test :replacement}}}
                            (second f))]
          (assert (= actual expected) (pr-str [s actual expected]))
          (prn s actual))))))
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'review]
    (ana/analyze (assoc (ana/empty-env) :ns {:name 'review :defs {} :use-macros {'pr160-provisional 'user}})
      (read-string "(def ^{:meta [:raw]} pending (pr160-provisional))"))))
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'cljs.core ana/*cljs-file* "native-core.sus"]
    (ana/analyze (assoc (ana/empty-env) :ns {:name 'cljs.core :defs {}}) '(def core-probe 7))
    (let [actual (select-keys (get-in @env/*compiler* [::ana/namespaces 'cljs.core :defs 'core-probe]) [:file :meta])]
      (assert (= actual {:file "cljs/core.cljs" :meta {:file "cljs/core.cljs"}}) (pr-str actual))
      (prn :core actual))))
