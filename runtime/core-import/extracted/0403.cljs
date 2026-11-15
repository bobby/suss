;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn iter [coll]
  (cond
    (iterable? coll) (-iterator coll)
    (nil? coll) (nil-iter)
    (string? coll) (string-iter coll)
    (array? coll) (array-iter coll)
    (seqable? coll) (seq-iter coll)
    :else (throw (js/Error. (str_ "Cannot create iterator from " coll)))))
