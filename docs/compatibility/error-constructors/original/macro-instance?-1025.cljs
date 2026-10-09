(core/defmacro instance? [c x]
  ;; Google Closure warns about some references to RegExp, so
  ;; (instance? RegExp ...) needs to be inlined, but the expansion
  ;; should preserve the order of argument evaluation.
  (bool-expr (if (clojure.core/symbol? c)
               (core/list 'js* "(~{} instanceof ~{})" x c)
               `(let [c# ~c x# ~x]
                  (~'js* "(~{} instanceof ~{})" x# c#)))))
