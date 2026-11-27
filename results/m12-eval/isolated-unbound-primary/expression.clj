(defmacro isolated-unbound-source [f]
 (eval '(declare isolated-unbound-target))
 (let [u (eval '(deref (var isolated-unbound-target)))] (list 'quote (f u))))

(prn [(eval (list 'user/isolated-unbound-source
 (fn [u] [(ifn? u) (fn? u) (var? (.-v u)) (identical? u @(.-v u))
  (try (u 1) (catch Throwable e (ex-message e)))])))
 (eval (list 'user/isolated-unbound-source
 (fn [u] (let [v (.-v u)]
  [(alter-var-root v (fn [old] (if (identical? old u) 42 -1))) @v
   (try (u) (catch Throwable e (ex-message e)))]))))]
)
