(prn (let [f (fn [] 42)]
 (try (eval (list 'def (with-meta 'catalog-meta-compile-failure {:macro f :payload f}) 'catalog-missing)) (catch Throwable _ nil))
 (let [v (eval '(var catalog-meta-compile-failure))] [(.isMacro v) (identical? f (:macro (meta v))) (identical? f (:payload (meta v)))])))
(prn (let [f (fn [] 42)]
 (try (eval (list 'def (with-meta 'catalog-meta-init-failure {:macro f :payload f}) '(throw (ex-info "first" {})))) (catch Throwable _ nil))
 (let [v (eval '(var catalog-meta-init-failure))] [(.isMacro v) (identical? f (:macro (meta v))) (identical? f (:payload (meta v)))])))
