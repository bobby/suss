;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(core/defn- typed-expr? [env form allowed-tags]
  (compatible? (cljs.analyzer/infer-tag env
                 (cljs.analyzer/no-warn (cljs.analyzer/analyze env form)))
    allowed-tags))

(core/defn- string-expr [e]
  (vary-meta e assoc :tag 'string))

(core/defmacro str_
  ([] "")
  ([x]
   (if (typed-expr? &env x '#{string})
     x
     (string-expr (core/list 'js* "cljs.core.str_(~{})" x))))
  ([x & ys]
   (core/let [interpolate (core/fn [x]
                            (if (typed-expr? &env x '#{string clj-nil})
                              "~{}"
                              "cljs.core.str_(~{})"))
              strs        (core/->> (core/list* x ys)
                            (map interpolate)
                            (interpose ",")
                            (apply core/str))]
     (string-expr (list* 'js* (core/str "[" strs "].join('')") x ys)))))

(core/defn- compile-time-constant? [x]
  (core/or
   (core/string? x)
   (core/keyword? x)
   (core/boolean? x)
   (core/number? x)))

(core/defmacro str
  [& xs]
  (core/let [interpolate (core/fn [x]
                           (core/cond
                             (typed-expr? &env x '#{clj-nil})
                             nil
                             (compile-time-constant? x)
                             ["+~{}" x]
                             :else
                             ;; Note: can't assume non-nil despite tag here, so we go through str 1-arity
                             ["+cljs.core.str.cljs$core$IFn$_invoke$arity$1(~{})" x]))
             strs+args (keep interpolate xs)
             strs (string/join (map first strs+args))
             args (map second strs+args)]
    (string-expr (list* 'js* (core/str "(\"\"" strs ")") args))))
