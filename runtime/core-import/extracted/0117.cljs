;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn compare-and-set!
  "Atomically sets the value of atom to newval if and only if the
  current value of the atom is equal to oldval. Returns true if
  set happened, else false."
  [^not-native a oldval newval]
  (if (= (-deref a) oldval)
    (do (reset! a newval) true)
    false))
