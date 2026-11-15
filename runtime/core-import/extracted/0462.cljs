;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype Single [^:mutable val]
  Object
  (add [this o]
    (if (identical? val NONE)
      (do
        (set! val o)
        this)
      (Many. #js [val o])))
  (remove [this]
    (if (identical? val NONE)
      (throw (js/Error. (str_ "Removing object from empty buffer")))
      (let [ret val]
        (set! val NONE)
        ret)))
  (isEmpty [this]
    (identical? val NONE))
  (toString [this]
    (str_ "Single: " val)))
