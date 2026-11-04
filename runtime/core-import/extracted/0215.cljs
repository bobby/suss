;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn apply
  "Applies fn f to the argument list formed by prepending intervening arguments to args."
  ([f args]
   (if (.-cljs$lang$applyTo f)
     (let [fixed-arity (.-cljs$lang$maxFixedArity f)
           bc (bounded-count (inc fixed-arity) args)]
       (if (<= bc fixed-arity)
         (apply-to f bc args)
         (.cljs$lang$applyTo f args)))
     (apply-to-simple f (seq args))))
  ([f x args]
   (if (.-cljs$lang$applyTo f)
     (let [arglist (list* x args)
           fixed-arity (.-cljs$lang$maxFixedArity f)
           bc (inc (bounded-count fixed-arity args))]
       (if (<= bc fixed-arity)
         (apply-to f bc arglist)
         (.cljs$lang$applyTo f arglist)))
     (apply-to-simple f x (seq args))))
  ([f x y args]
   (if (.-cljs$lang$applyTo f)
     (let [arglist (list* x y args)
           fixed-arity (.-cljs$lang$maxFixedArity f)
           bc (+ 2 (bounded-count (dec fixed-arity) args))]
       (if (<= bc fixed-arity)
         (apply-to f bc arglist)
         (.cljs$lang$applyTo f arglist)))
     (apply-to-simple f x y (seq args))))
  ([f x y z args]
   (if (.-cljs$lang$applyTo f)
     (let [arglist (list* x y z args)
           fixed-arity (.-cljs$lang$maxFixedArity f)
           bc (+ 3 (bounded-count (- fixed-arity 2) args))]
       (if (<= bc fixed-arity)
         (apply-to f bc arglist)
         (.cljs$lang$applyTo f arglist)))
     (apply-to-simple f x y z (seq args))))
  ([f a b c d & args]
   (if (.-cljs$lang$applyTo f)
     (let [spread-args (spread args)
           arglist (cons a (cons b (cons c (cons d spread-args))))
           fixed-arity (.-cljs$lang$maxFixedArity f)
           bc (+ 4 (bounded-count (- fixed-arity 3) spread-args))]
       (if (<= bc fixed-arity)
         (apply-to f bc arglist)
         (.cljs$lang$applyTo f arglist)))
     (apply-to-simple f a b c d (spread args)))))
