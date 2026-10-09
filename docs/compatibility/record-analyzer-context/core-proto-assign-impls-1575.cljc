(core/defn- proto-assign-impls [env resolve type-sym type [p sigs]]
  (update-protocol-var p type env)
  (core/let [psym      (resolve p)
             pprefix   (protocol-prefix psym)
             skip-flag (set (core/-> type-sym meta :skip-protocol-flag))]
    (if (= p 'Object)
      (add-obj-methods type type-sym sigs)
      (concat
        (core/when-not (skip-flag psym)
          [`(set! ~(extend-prefix type-sym pprefix) cljs.core/PROTOCOL_SENTINEL)])
        (mapcat
          (core/fn [sig]
            (if (= psym 'cljs.core/IFn)
              (add-ifn-methods type type-sym sig)
              (add-proto-methods* pprefix type type-sym sig)))
          sigs)))))
