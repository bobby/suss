;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn swap!
  "Atomically swaps the value of atom to be:
  (apply f current-value-of-atom args). Note that f may be called
  multiple times, and thus should be free of side effects.  Returns
  the value that was swapped in."
  ([a f]
   (if (instance? Atom a)
     (reset! a (f (.-state a)))
     (-swap! a f)))
  ([a f x]
   (if (instance? Atom a)
     (reset! a (f (.-state a) x))
     (-swap! a f x)))
  ([a f x y]
   (if (instance? Atom a)
     (reset! a (f (.-state a) x y))
     (-swap! a f x y)))
  ([a f x y & more]
   (if (instance? Atom a)
     (reset! a (apply f (.-state a) x y more))
     (-swap! a f x y more))))
