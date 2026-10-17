(core/defn destructure [bindings]
  (core/let [bents (partition 2 bindings)
             pb (core/fn pb [bvec b v]
                  (core/let [pvec
                             (core/fn [bvec b val]
                               (core/let [gvec (gensym "vec__")
                                          gseq (gensym "seq__")
                                          gfirst (gensym "first__")
                                          has-rest (some #{'&} b)]
                                 (core/loop [ret (core/let [ret (conj bvec gvec val)]
                                                   (if has-rest
                                                     (conj ret gseq (core/list `seq gvec))
                                                     ret))
                                             n 0
                                             bs b
                                             seen-rest? false]
                                   (if (seq bs)
                                     (core/let [firstb (first bs)]
                                       (core/cond
                                         (= firstb '&) (recur (pb ret (second bs) gseq)
                                                              n
                                                              (nnext bs)
                                                              true)
                                         (= firstb :as) (pb ret (second bs) gvec)
                                         :else (if seen-rest?
                                                 (throw #?(:clj (new Exception "Unsupported binding form, only :as can follow & parameter")
                                                           :cljs (new js/Error "Unsupported binding form, only :as can follow & parameter")))
                                                 (recur (pb (if has-rest
                                                              (conj ret
                                                                    gfirst `(first ~gseq)
                                                                    gseq `(next ~gseq))
                                                              ret)
                                                            firstb
                                                            (if has-rest
                                                              gfirst
                                                              (core/list `nth gvec n nil)))
                                                        (core/inc n)
                                                        (next bs)
                                                        seen-rest?))))
                                     ret))))
                             pmap
                             (core/fn [bvec b v]
                               (core/let [gmap (gensym "map__")
                                          defaults (:or b)]
                                 (core/loop [ret (core/-> bvec (conj gmap) (conj v)
                                                          (conj gmap) (conj `(--destructure-map ~gmap))
                                                     ((core/fn [ret]
                                                        (if (:as b)
                                                          (conj ret (:as b) gmap)
                                                          ret))))
                                             bes (core/let [transforms
                                                            (reduce
                                                              (core/fn [transforms mk]
                                                                (if (core/keyword? mk)
                                                                  (core/let [mkns (namespace mk)
                                                                        mkn (name mk)]
                                                                    (core/cond (= mkn "keys") (assoc transforms mk #(keyword (core/or mkns (namespace %)) (name %)))
                                                                               (= mkn "syms") (assoc transforms mk #(core/list `quote (symbol (core/or mkns (namespace %)) (name %))))
                                                                               (= mkn "strs") (assoc transforms mk core/str)
                                                                               :else transforms))
                                                                  transforms))
                                                              {}
                                                              (keys b))]
                                                   (reduce
                                                     (core/fn [bes entry]
                                                       (reduce #(assoc %1 %2 ((val entry) %2))
                                                         (dissoc bes (key entry))
                                                         ((key entry) bes)))
                                                     (dissoc b :as :or)
                                                     transforms))]
                                   (if (seq bes)
                                     (core/let [bb (key (first bes))
                                                bk (val (first bes))
                                                local (if #?(:clj  (core/instance? clojure.lang.Named bb)
                                                             :cljs (cljs.core/implements? INamed bb))
                                                          (with-meta (symbol nil (name bb)) (meta bb))
                                                        bb)
                                                bv (if (contains? defaults local)
                                                     (core/list 'cljs.core/get gmap bk (defaults local))
                                                     (core/list 'cljs.core/get gmap bk))]
                                       (recur
                                         (if (core/or (core/keyword? bb) (core/symbol? bb)) ;(ident? bb)
                                           (core/-> ret (conj local bv))
                                           (pb ret bb bv))
                                              (next bes)))
                                     ret))))]
                    (core/cond
                      (core/symbol? b) (core/-> bvec (conj (if (namespace b) (symbol (name b)) b)) (conj v))
                      (core/keyword? b) (core/-> bvec (conj (symbol (name b))) (conj v))
                      (vector? b) (pvec bvec b v)
                      (map? b) (pmap bvec b v)
                      :else (throw
                             #?(:clj (new Exception (core/str "Unsupported binding form: " b))
                                :cljs (new js/Error (core/str "Unsupported binding form: " b)))))))
             process-entry (core/fn [bvec b] (pb bvec (first b) (second b)))]
    (if (every? core/symbol? (map first bents))
      bindings
      (core/if-let [kwbs (seq (filter #(core/keyword? (first %)) bents))]
        (throw
          #?(:clj (new Exception (core/str "Unsupported binding key: " (ffirst kwbs)))
             :cljs (new js/Error (core/str "Unsupported binding key: " (ffirst kwbs)))))
        (reduce process-entry [] bents)))))
