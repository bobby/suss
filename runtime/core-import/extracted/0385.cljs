;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn ^number compare
  "Comparator. Returns a negative number, zero, or a positive number
  when x is logically 'less than', 'equal to', or 'greater than'
  y. Uses IComparable if available and google.array.defaultCompare for objects
 of the same type and special-cases nil to be less than any other object."
  [x y]
  (cond
   (identical? x y) 0

   (nil? x) -1

   (nil? y) 1

   (number? x) (if (number? y)
                 (garray/defaultCompare x y)
                 (throw (js/Error. (str_ "Cannot compare " x " to " y))))

   (satisfies? IComparable x)
   (-compare x y)

   :else
   (if (and (or (string? x) (array? x) (true? x) (false? x))
            (identical? (type x) (type y)))
     (garray/defaultCompare x y)
     (throw (js/Error. (str_ "Cannot compare " x " to " y))))))
