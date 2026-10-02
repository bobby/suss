;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(def ^:private cs (into [] (map (comp gensym core/str core/char) (range 97 118))))

(core/defn- gen-apply-to-helper
  ([] (gen-apply-to-helper 1))
  ([n]
   (if (core/<= n 20)
     `(let [~(cs (core/dec n)) (-first ~'args)
            ~'args (-rest ~'args)]
        (if (== ~'argc ~n)
          (~'f ~@(take n cs))
          ~(gen-apply-to-helper (core/inc n))))
     `(throw (js/Error. "Only up to 20 arguments supported on functions")))))

(core/defmacro gen-apply-to []
  `(do
     (set! ~'*unchecked-if* true)
     (defn ~'apply-to [~'f ~'argc ~'args]
       (let [~'args (seq ~'args)]
         (if (zero? ~'argc)
           (~'f)
           ~(gen-apply-to-helper))))
     (set! ~'*unchecked-if* false)))

(core/defn- gen-apply-to-simple-helper
  [f num-args args]
  (core/let [new-arg-sym (symbol (core/str "a" num-args))
             proto-name (core/str "cljs$core$IFn$_invoke$arity$" (core/inc num-args))
             proto-prop (symbol (core/str ".-" proto-name))
             proto-inv (symbol (core/str "." proto-name))
             next-sym (symbol (core/str "next_" num-args))
             all-args (mapv #(symbol (core/str "a" %)) (range (core/inc num-args)))]
    `(let [~new-arg-sym (cljs.core/-first ~args)
           ~next-sym (cljs.core/next ~args)]
       (if (nil? ~next-sym)
         (if (~proto-prop ~f)
           (~proto-inv ~f ~@all-args)
           (.call ~f ~f ~@all-args))
         ~(if (core/<= 19 num-args)
            ;; We've exhausted all protocols, fallback to .apply:
            `(let [arr# (cljs.core/array ~@all-args)]
               (loop [s# ~next-sym]
                 (when s#
                   (do (.push arr# (cljs.core/-first s#))
                       (recur (cljs.core/next s#)))))
               (.apply ~f ~f arr#))
            (gen-apply-to-simple-helper f (core/inc num-args) next-sym))))))

(core/defmacro gen-apply-to-simple
  [f num-args args]
  (gen-apply-to-simple-helper f num-args args))
