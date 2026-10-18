(defmacro assert-args
  "Internal - do not use!"
  [fnname & pairs]
  `(do (when-not ~(first pairs)
         (throw (ex-info ~(str fnname " requires " (second pairs)) {:clojure.error/phase :macro-syntax-check})))
       ~(let [more (nnext pairs)]
          (when more
            (list* `assert-args fnname more)))))
