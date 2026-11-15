;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn reset!
  "Sets the value of atom to newval without regard for the
  current value. Returns new-value."
  [a new-value]
  (if (instance? Atom a)
    (let [validate (.-validator a)]
      (when-not (nil? validate)
        (when-not (validate new-value)
          (throw (js/Error. "Validator rejected reference state"))))
      (let [old-value (.-state a)]
        (set! (.-state a) new-value)
        (when-not (nil? (.-watches a))
          (-notify-watches a old-value new-value))
        new-value))
    (-reset! a new-value)))
