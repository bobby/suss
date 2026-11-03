;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- apply-to-simple
  "Internal. DO NOT USE!
  Assumes args was already called with seq beforehand!"
  ([f ^seq args]
   (if (nil? args)
     (if (.-cljs$core$IFn$_invoke$arity$0 f)
       (.cljs$core$IFn$_invoke$arity$0 f)
       (.call f f))
     (apply-to-simple f (-first args) (next* args))))
  ([f a0 ^seq args]
   (if (nil? args)
     (if (.-cljs$core$IFn$_invoke$arity$1 f)
       (.cljs$core$IFn$_invoke$arity$1 f a0)
       (.call f f a0))
     (apply-to-simple f a0 (-first args) (next* args))))
  ([f a0 a1 ^seq args]
   (if (nil? args)
     (if (.-cljs$core$IFn$_invoke$arity$2 f)
       (.cljs$core$IFn$_invoke$arity$2 f a0 a1)
       (.call f f a0 a1))
     (apply-to-simple f a0 a1 (-first args) (next* args))))
  ([f a0 a1 a2 ^seq args]
   (if (nil? args)
     (if (.-cljs$core$IFn$_invoke$arity$3 f)
       (.cljs$core$IFn$_invoke$arity$3 f a0 a1 a2)
       (.call f f a0 a1 a2))
     (apply-to-simple f a0 a1 a2 (-first args) (next* args))))
  ([f a0 a1 a2 a3 ^seq args]
   (if (nil? args)
     (if (.-cljs$core$IFn$_invoke$arity$4 f)
       (.cljs$core$IFn$_invoke$arity$4 f a0 a1 a2 a3)
       (.call f f a0 a1 a2 a3))
     (gen-apply-to-simple f 4 args))))
