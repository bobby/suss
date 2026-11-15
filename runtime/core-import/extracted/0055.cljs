;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn cons
  "Returns a new seq where x is the first element and coll is the rest."
  [x coll]
  (cond
    (nil? coll)             (List. nil x nil 1 nil)
    (implements? ISeq coll) (Cons. nil x coll nil)
    :default                (Cons. nil x (seq coll) nil)))
