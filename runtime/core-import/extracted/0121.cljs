;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn swap-vals!
  "Atomically swaps the value of atom to be:
  (apply f current-value-of-atom args). Note that f may be called
  multiple times, and thus should be free of side effects.
  Returns [old new], the value of the atom before and after the swap."
  {:added "1.9"}
  ([a f]
   (if (instance? Atom a)
     (reset-vals! a (f (.-state a)))
     [(-deref a) (-swap! a f)]))
  ([a f x]
   (if (instance? Atom a)
     (reset-vals! a (f (.-state a) x))
     [(-deref a) (-swap! a f x)]))
  ([a f x y]
   (if (instance? Atom a)
     (reset-vals! a (f (.-state a) x y))
     [(-deref a) (-swap! a f x y)]))
  ([a f x y & more]
   (if (instance? Atom a)
     (reset-vals! a (apply f (.-state a) x y more))
     [(-deref a) (-swap! a f x y more)])))
