;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn ^:private fn->comparator
  "Given a fn that might be boolean valued or a comparator,
   return a fn that is a comparator."
  [f]
  (if (= f compare)
    compare
    (fn [x y]
      (let [r (f x y)]
        (if (number? r)
          r
          (if r
            -1
            (if (f y x) 1 0)))))))
