;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn juxt
  "Takes a set of functions and returns a fn that is the juxtaposition
  of those fns.  The returned fn takes a variable number of args, and
  returns a vector containing the result of applying each fn to the
  args (left-to-right).
  ((juxt a b c) x) => [(a x) (b x) (c x)]"
  ([f]
     (fn
       ([] (vector (f)))
       ([x] (vector (f x)))
       ([x y] (vector (f x y)))
       ([x y z] (vector (f x y z)))
       ([x y z & args] (vector (apply f x y z args)))))
  ([f g]
     (fn
       ([] (vector (f) (g)))
       ([x] (vector (f x) (g x)))
       ([x y] (vector (f x y) (g x y)))
       ([x y z] (vector (f x y z) (g x y z)))
       ([x y z & args] (vector (apply f x y z args) (apply g x y z args)))))
  ([f g h]
     (fn
       ([] (vector (f) (g) (h)))
       ([x] (vector (f x) (g x) (h x)))
       ([x y] (vector (f x y) (g x y) (h x y)))
       ([x y z] (vector (f x y z) (g x y z) (h x y z)))
       ([x y z & args] (vector (apply f x y z args) (apply g x y z args) (apply h x y z args)))))
  ([f g h & fs]
     (let [fs (list* f g h fs)]
       (fn
         ([] (reduce #(conj %1 (%2)) [] fs))
         ([x] (reduce #(conj %1 (%2 x)) [] fs))
         ([x y] (reduce #(conj %1 (%2 x y)) [] fs))
         ([x y z] (reduce #(conj %1 (%2 x y z)) [] fs))
         ([x y z & args] (reduce #(conj %1 (apply %2 x y z args)) [] fs))))))
